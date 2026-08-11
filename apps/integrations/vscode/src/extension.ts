import * as vscode from "vscode";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

// `typedmark-lsp` has no published release binary yet (see
// `apps/typedmark-lsp`), so like `apps/zed-extension` this doesn't
// download or bundle one -- it expects the binary already on `$PATH`
// (e.g. via the nix package), configurable via `typedmark.serverPath`
// for anyone running it from a non-standard location.
export function activate(context: vscode.ExtensionContext): void {
	const config = vscode.workspace.getConfiguration("typedmark");
	const command = config.get<string>("serverPath", "typedmark-lsp");

	const serverOptions: ServerOptions = {
		command,
		args: [],
	};

	const clientOptions: LanguageClientOptions = {
		documentSelector: [{ scheme: "file", language: "typedmark" }],
	};

	client = new LanguageClient(
		"typedmarkLanguageServer",
		"TypedMark Language Server",
		serverOptions,
		clientOptions,
	);

	client.start().then(undefined, (err: unknown) => {
		vscode.window.showErrorMessage(
			`Failed to start typedmark-lsp ("${command}"): ${err}. ` +
				`Install it on $PATH or set the "typedmark.serverPath" setting.`,
		);
	});

	context.subscriptions.push({ dispose: () => client?.stop() });
}

export function deactivate(): Thenable<void> | undefined {
	return client?.stop();
}
