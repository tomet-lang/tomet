import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function resolveServerPath(configuredPath: string): string {
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

	// 4. Fallback: check workspace target/debug/typedmark-lsp or target/release/typedmark-lsp
	if (workspaceFolder) {
		const debugPath = path.join(workspaceFolder, "target", "debug", "typedmark-lsp");
		if (fs.existsSync(debugPath)) {
			return debugPath;
		}
		const releasePath = path.join(workspaceFolder, "target", "release", "typedmark-lsp");
		if (fs.existsSync(releasePath)) {
			return releasePath;
		}
		const parentDebugPath = path.resolve(workspaceFolder, "..", "..", "..", "target", "debug", "typedmark-lsp");
		if (fs.existsSync(parentDebugPath)) {
			return parentDebugPath;
		}
	}

	return resolved;
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
	context.subscriptions.push(
		vscode.commands.registerCommand("typedmark.restartServer", async () => {
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
	const config = vscode.workspace.getConfiguration("typedmark");
	const rawCommand = config.get<string>("serverPath", "typedmark-lsp");
	const command = resolveServerPath(rawCommand);

	const serverOptions: ServerOptions = {
		command,
		args: [],
	};

	const clientOptions: LanguageClientOptions = {
		documentSelector: [
			{ scheme: "file", language: "typedmark" },
			{ scheme: "untitled", language: "typedmark" },
		],
	};

	client = new LanguageClient(
		"typedmarkLanguageServer",
		"TypedMark Language Server",
		serverOptions,
		clientOptions
	);

	try {
		await client.start();
	} catch (err: unknown) {
		vscode.window.showErrorMessage(
			`Failed to start typedmark-lsp ("${command}"): ${err}. ` +
				`Install it on $PATH or set the "typedmark.serverPath" setting.`
		);
	}
}

export function deactivate(): Thenable<void> | undefined {
	return client?.stop();
}
