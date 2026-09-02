# svfmt (VS Code extension)

Registers `svfmt` as a formatter for `.sv`, `.svh`, `.v` and `.vh` files. On "Format
Document" (or `editor.formatOnSave`), the buffer is piped through `svfmt --stdout -`
and replaced with the result; on failure the buffer is left untouched and the error is
shown.

Plain JavaScript, no build step, no runtime dependencies. Packaging (below) shells out
to `@vscode/vsce` via `npx` rather than adding it as a project dependency.

## Requirements

`svfmt` must be on `PATH` (`cargo install --path .` from the repo root, or add
`target/release` to `PATH` after `cargo build --release`), or set `svfmt.path` to its
location.

Note: formatting here always runs as if via stdin, so it does **not** honor the
`[tool.sv-format] exclude` list in `pyproject.toml`/`sv-format.toml` (there's no file
path to match against). That list only matters for the CLI / pre-commit invocation.

## Install (local, unpublished)

```bash
cd tools/vscode-svfmt
npx --yes @vscode/vsce package --allow-missing-repository --skip-license -o svfmt.vsix
code --install-extension svfmt.vsix --force
```

Re-run this after editing `extension.js` to pick up changes -- then reload VS Code.

Packaging and installing through `code --install-extension` (rather than symlinking the
folder into `~/.vscode/extensions`) matters: VS Code's startup extension GC deletes
anything in that directory that isn't registered in its own `extensions.json`, so a bare
symlinked/copied folder gets silently removed on the next restart. A real install is
exempt.

Or, to try changes without installing anything, open this folder in VS Code and press F5
to launch an Extension Development Host window with it loaded (that path also isn't
subject to the GC, since it's never copied into `~/.vscode/extensions` at all).

## Settings

| Setting      | Default | Description                                                        |
|--------------|---------|----------------------------------------------------------------------|
| `svfmt.path` | `svfmt` | Path to the executable.                                            |
| `svfmt.args` | `[]`    | Extra args inserted before `--stdout -`, e.g. `["--indent", "4"]`. |

To format on save, add to your `settings.json`:

```json
{
  "editor.formatOnSave": true,
  "[systemverilog]": { "editor.defaultFormatter": "svfmt.svfmt" },
  "[verilog]": { "editor.defaultFormatter": "svfmt.svfmt" }
}
```

(Only needed if another extension also registers a formatter for those languages and you
want to force this one.)
