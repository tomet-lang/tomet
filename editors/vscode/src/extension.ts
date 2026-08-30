import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let activeExtensionPath: string | undefined;

function resolveServerPath(configuredPath: string, extensionPath: string): string {
	const workspaceFolders = vscode.workspace.workspaceFolders;
	const workspaceFolder = workspaceFolders && workspaceFolders.length > 0
		? workspaceFolders[0].uri.fsPath
		: undefined;

	let resolved = configuredPath;

	// 1. Expand ${workspaceFolder} if present
	if (workspaceFolder && resolved.includes("${workspaceFolder}")) {
		resolved = resolved.replace(/\${workspaceFolder}/g, workspaceFolder);
	}

	// 2. Expand ~ if present
	if (resolved.startsWith("~")) {
		const home = process.env.HOME || process.env.USERPROFILE || "";
		resolved = path.join(home, resolved.slice(1));
	}

	// 3. If resolved path exists on filesystem, return it
	if (fs.existsSync(resolved)) {
		return resolved;
	}

	// 4. Fallback: check workspace target/debug/tomet-lsp or target/release/tomet-lsp
	if (workspaceFolder) {
		const debugPath = path.join(workspaceFolder, "target", "debug", "tomet-lsp");
		if (fs.existsSync(debugPath)) {
			return debugPath;
		}
		const releasePath = path.join(workspaceFolder, "target", "release", "tomet-lsp");
		if (fs.existsSync(releasePath)) {
			return releasePath;
		}
		const parentDebugPath = path.resolve(workspaceFolder, "..", "..", "target", "debug", "tomet-lsp");
		if (fs.existsSync(parentDebugPath)) {
			return parentDebugPath;
		}
		const parentReleasePath = path.resolve(workspaceFolder, "..", "..", "target", "release", "tomet-lsp");
		if (fs.existsSync(parentReleasePath)) {
			return parentReleasePath;
		}
	}

	// 5. Fallback: locate the repo checkout this extension itself was loaded from
	// (handles running via F5 with no folder open in the Extension Development Host).
	// extensionPath is .../editors/vscode; the repo root is two levels up.
	const repoRoot = path.resolve(extensionPath, "..", "..");
	const extDebugPath = path.join(repoRoot, "target", "debug", "tomet-lsp");
	if (fs.existsSync(extDebugPath)) {
		return extDebugPath;
	}
	const extReleasePath = path.join(repoRoot, "target", "release", "tomet-lsp");
	if (fs.existsSync(extReleasePath)) {
		return extReleasePath;
	}

	return resolved;
}

const codeSpanDecorationType = vscode.window.createTextEditorDecorationType({
	backgroundColor: "transparent",
	border: "1px solid #4fc1ff",
	borderRadius: "3px",
});

const DEFAULT_BRACKET_THEME_COLORS: (string | vscode.ThemeColor)[] = [
	new vscode.ThemeColor("editorBracketHighlight.foreground1"),
	new vscode.ThemeColor("editorBracketHighlight.foreground2"),
	new vscode.ThemeColor("editorBracketHighlight.foreground3"),
];

let tableColumnDecorationTypes: vscode.TextEditorDecorationType[] = [];

function initTableColumnDecorations(context: vscode.ExtensionContext): void {
	for (const dt of tableColumnDecorationTypes) {
		dt.dispose();
	}
	tableColumnDecorationTypes = [];

	const config = vscode.workspace.getConfiguration("tomet");
	const customColors = config.get<string[]>("table.columnColors", []);
	const colors = customColors && customColors.length > 0
		? customColors
		: DEFAULT_BRACKET_THEME_COLORS;

	for (const color of colors) {
		const dt = vscode.window.createTextEditorDecorationType({
			borderWidth: "0 0 1px 0",
			borderStyle: "dashed",
			borderColor: color,
		});
		tableColumnDecorationTypes.push(dt);
		context.subscriptions.push(dt);
	}
}

function extractTableRowCells(lineText: string, lineIndex: number): vscode.Range[] {
	const ranges: vscode.Range[] = [];
	let i = 0;
	while (i < lineText.length) {
		if (lineText[i] === "[") {
			const openBracketIdx = i;
			let depth = 1;
			i++;
			while (i < lineText.length && depth > 0) {
				if (lineText[i] === "[") {
					depth++;
				} else if (lineText[i] === "]") {
					depth--;
				}
				i++;
			}
			if (depth === 0) {
				const closeBracketIdx = i - 1;
				const innerText = lineText.slice(openBracketIdx + 1, closeBracketIdx);
				const leadingSpaces = innerText.length - innerText.trimStart().length;
				const trailingSpaces = innerText.length - innerText.trimEnd().length;
				const startChar = openBracketIdx + 1 + leadingSpaces;
				const endChar = closeBracketIdx - trailingSpaces;
				if (endChar > startChar) {
					ranges.push(new vscode.Range(lineIndex, startChar, lineIndex, endChar));
				} else {
					ranges.push(new vscode.Range(lineIndex, openBracketIdx + 1, lineIndex, closeBracketIdx));
				}
			}
		} else {
			i++;
		}
	}
	return ranges;
}

function updateDecorations(editor: vscode.TextEditor | undefined): void {
	if (!editor) {
		return;
	}
	const langId = editor.document.languageId;
	if (langId !== "tomet" && langId !== "markdown") {
		return;
	}

	const doc = editor.document;
	const lineCount = doc.lineCount;

	// 1. Code span decorations (`...`)
	const text = doc.getText();
	const regex = /`[^`\r\n]+`/g;
	const codeDecorations: vscode.DecorationOptions[] = [];
	let match: RegExpExecArray | null;

	while ((match = regex.exec(text)) !== null) {
		const startPos = doc.positionAt(match.index);
		const endPos = doc.positionAt(match.index + match[0].length);
		codeDecorations.push({ range: new vscode.Range(startPos, endPos) });
	}

	editor.setDecorations(codeSpanDecorationType, codeDecorations);

	// 2. Table column underline / dotted decorations
	const colDecorations: vscode.DecorationOptions[][] = tableColumnDecorationTypes.map(() => []);

	let inTable = false;
	let pendingTableHeader = false;
	for (let lineIdx = 0; lineIdx < lineCount; lineIdx++) {
		const line = doc.lineAt(lineIdx).text;
		const trimmed = line.trim();

		if (!inTable) {
			if (
				trimmed.startsWith("@table[") ||
				trimmed.startsWith("<table[") ||
				(trimmed.startsWith("@table") && trimmed.includes("[")) ||
				(trimmed.startsWith("<table") && trimmed.includes("["))
			) {
				inTable = true;
				pendingTableHeader = false;
			} else if (trimmed.startsWith("@table") || trimmed.startsWith("<table")) {
				pendingTableHeader = true;
			} else if (pendingTableHeader) {
				if (trimmed.includes("[")) {
					inTable = true;
					pendingTableHeader = false;
				} else if (!trimmed.startsWith("(") && !trimmed.endsWith(")") && !trimmed.includes(":")) {
					pendingTableHeader = false;
				}
			}
		} else {
			if (trimmed === "]" || trimmed.startsWith("]{") || trimmed.startsWith("] ")) {
				inTable = false;
			} else {
				const cellRanges = extractTableRowCells(line, lineIdx);
				for (let c = 0; c < cellRanges.length; c++) {
					const decIdx = c % tableColumnDecorationTypes.length;
					colDecorations[decIdx].push({ range: cellRanges[c] });
				}
			}
		}
	}

	for (let d = 0; d < tableColumnDecorationTypes.length; d++) {
		editor.setDecorations(tableColumnDecorationTypes[d], colDecorations[d]);
	}
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
	activeExtensionPath = context.extensionPath;
	context.subscriptions.push(codeSpanDecorationType);
	initTableColumnDecorations(context);

	vscode.workspace.onDidChangeConfiguration((e) => {
		if (e.affectsConfiguration("tomet.table.columnColors")) {
			initTableColumnDecorations(context);
			if (vscode.window.activeTextEditor) {
				updateDecorations(vscode.window.activeTextEditor);
			}
		}
	}, null, context.subscriptions);

	vscode.window.onDidChangeActiveTextEditor((editor) => {
		updateDecorations(editor);
	}, null, context.subscriptions);

	vscode.workspace.onDidChangeTextDocument((event) => {
		if (vscode.window.activeTextEditor && event.document === vscode.window.activeTextEditor.document) {
			updateDecorations(vscode.window.activeTextEditor);
		}
	}, null, context.subscriptions);

	if (vscode.window.activeTextEditor) {
		updateDecorations(vscode.window.activeTextEditor);
	}

	context.subscriptions.push(
		vscode.commands.registerCommand("tomet.restartServer", async () => {
			if (client) {
				await client.stop();
				client = undefined;
			}
			await startClient();
		})
	);

	await startClient();
}

async function startClient(): Promise<void> {
	const config = vscode.workspace.getConfiguration("tomet");
	const rawCommand = config.get<string>("serverPath", "tomet-lsp");
	const command = resolveServerPath(rawCommand, activeExtensionPath as string);

	const serverOptions: ServerOptions = {
		command,
		args: [],
	};

	const clientOptions: LanguageClientOptions = {
		documentSelector: [
			{ scheme: "file", language: "tomet" },
			{ scheme: "untitled", language: "tomet" },
		],
	};

	client = new LanguageClient(
		"tometLanguageServer",
		"Tomet Language Server",
		serverOptions,
		clientOptions
	);

	try {
		await client.start();
	} catch (err: unknown) {
		vscode.window.showErrorMessage(
			`Failed to start tomet-lsp ("${command}"): ${err}. ` +
				`Install it on $PATH or set the "tomet.serverPath" setting.`
		);
	}
}

export function deactivate(): Thenable<void> | undefined {
	return client?.stop();
}
