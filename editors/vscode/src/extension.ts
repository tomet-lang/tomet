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

function updateDecorations(editor: vscode.TextEditor | undefined): void {
	if (!editor) {
		return;
	}
	const langId = editor.document.languageId;
	if (langId !== "tomet" && langId !== "markdown") {
		return;
	}

	const text = editor.document.getText();
	const regex = /`[^`\r\n]+`/g;
	const decorations: vscode.DecorationOptions[] = [];
	let match: RegExpExecArray | null;

	while ((match = regex.exec(text)) !== null) {
		const startPos = editor.document.positionAt(match.index);
		const endPos = editor.document.positionAt(match.index + match[0].length);
		decorations.push({ range: new vscode.Range(startPos, endPos) });
	}

	editor.setDecorations(codeSpanDecorationType, decorations);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
	activeExtensionPath = context.extensionPath;
	context.subscriptions.push(codeSpanDecorationType);

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
