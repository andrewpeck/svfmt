'use strict';

// Wraps svfmt as a VS Code DocumentFormattingEditProvider. The buffer text is
// piped through `svfmt --stdout -` and the whole document is replaced with
// its stdout; on any failure the buffer is left untouched. Note that this
// bypasses the [tool.sv-format] exclude list (stdin formatting has no path
// to match against) -- that list is only enforced by the CLI invocation.

const vscode = require('vscode');
const { spawn } = require('child_process');

const DOCUMENT_SELECTOR = { pattern: '**/*.{sv,svh,v,vh}' };

function runFormatter(text, token) {
  const config = vscode.workspace.getConfiguration('svfmt');
  const exe = config.get('path', 'svfmt');
  const args = [...config.get('args', []), '--stdout', '-'];

  return new Promise((resolve, reject) => {
    const proc = spawn(exe, args);
    const stdout = [];
    const stderr = [];

    proc.stdout.on('data', (chunk) => stdout.push(chunk));
    proc.stderr.on('data', (chunk) => stderr.push(chunk));
    proc.on('error', (error) => reject(new Error(`could not run "${exe}": ${error.message}`)));
    proc.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(Buffer.concat(stderr).toString('utf8').trim() || `exited with code ${code}`));
        return;
      }
      resolve(Buffer.concat(stdout).toString('utf8'));
    });

    token.onCancellationRequested(() => proc.kill());
    proc.stdin.end(text, 'utf8');
  });
}

async function provideDocumentFormattingEdits(document, _options, token) {
  const original = document.getText();

  let formatted;
  try {
    formatted = await runFormatter(original, token);
  } catch (error) {
    vscode.window.showErrorMessage(`svfmt: ${error.message}`);
    return [];
  }

  if (formatted === original) {
    return [];
  }

  const fullRange = new vscode.Range(document.positionAt(0), document.positionAt(original.length));
  return [vscode.TextEdit.replace(fullRange, formatted)];
}

function activate(context) {
  context.subscriptions.push(
    vscode.languages.registerDocumentFormattingEditProvider(DOCUMENT_SELECTOR, { provideDocumentFormattingEdits })
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
