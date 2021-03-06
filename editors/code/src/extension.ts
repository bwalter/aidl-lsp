import * as path from 'path';
import * as vscode from 'vscode';

import {
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

function getProcessName(): string | null {
    if (process.platform === "darwin") {
        return "aidl-lsp-x86_64-apple-darwin";
    }

    if (process.platform === "linux") {
        if (process.arch === "x64") {
            return "aidl-lsp-x86_64-unknown-linux-gnu";
        }

        console.error("Unsupported linux arch:", process.arch);
        return;
    }

    if (process.platform === "win32") {
        return "aidl-lsp-x86_64-pc-windows-gnu.exe";
    }

    console.error("Unsupported platform:", process.platform);
}

export async function activate(context: vscode.ExtensionContext) {
	// Local aidl-lsp (TODO: published path)
	//const serverModule = context.asAbsolutePath(
	//	path.join('..', '..', 'target', 'debug', 'aidl-lsp')
	//);

  // Package aidl-lsp
  const packageName = getProcessName();
  if (getProcessName() == null) {
      return;
  }
  const serverModule = path.join(context.extensionPath, 'bin', getProcessName());

	// If the extension is launched in debug mode then the debug server options are used
	// Otherwise the run options are used
	const serverOptions: ServerOptions = {
		run: { command: serverModule },
		debug: { command: serverModule },
	};

	// Options to control the language client
	const clientOptions: LanguageClientOptions = {
		// Register the server for plain text documents
		documentSelector: [{ scheme: 'file', language: 'aidl' }],
	};

	// Create the language client and start the client.
	client = new LanguageClient(
		'aidl-lsp',
		'AIDL LSP',
		serverOptions,
		clientOptions
	);

	const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left);
	statusBar.text = "rust-analyzer";
	statusBar.tooltip = "ready";
	statusBar.command = "rust-analyzer.analyzerStatus";
	statusBar.show();

	client.start();
	await client.onReady();
	//client.onNotification(ra.serverStatus, (params) => res.setServerStatus(params));
	//return res;


	// Start the client. This will also launch the server
	//client.start();

}

export function deactivate(): Thenable<void> | undefined {
	if (!client) {
		return undefined;
	}
	return client.stop();
}
