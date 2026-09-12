# rcsv

Terminal CSV editor. One job: edit cell values in a CSV file, in a spreadsheet-like TUI.

## Download

### Linux

**x86_64**

```bash
curl -fsSL https://github.com/aleee-dev1/rcsv/releases/latest/download/rcsv-linux-x86_64 -o ~/.local/bin/rcsv
chmod +x ~/.local/bin/rcsv
```

**ARM64**

```bash
curl -fsSL https://github.com/aleee-dev1/rcsv/releases/latest/download/rcsv-linux-aarch64 -o ~/.local/bin/rcsv
chmod +x ~/.local/bin/rcsv
```

### macOS

**Apple Silicon**

```bash
curl -fsSL https://github.com/aleee-dev1/rcsv/releases/latest/download/rcsv-macos-aarch64 -o ~/.local/bin/rcsv
chmod +x ~/.local/bin/rcsv
```

**Intel**

```bash
curl -fsSL https://github.com/aleee-dev1/rcsv/releases/latest/download/rcsv-macos-x86_64 -o ~/.local/bin/rcsv
chmod +x ~/.local/bin/rcsv
```

### One-command install

```bash
curl -fsSL https://raw.githubusercontent.com/aleee-dev1/rcsv/main/install.sh | bash
```

The installer automatically detects your OS and architecture and downloads the appropriate binary.

### Build from source

```bash
cargo install --git https://github.com/aleee-dev1/rcsv
```

## Usage

```bash
rcsv <file.csv>
```

- If `<file.csv>` doesn't exist, `rcsv` opens with a single empty cell; saving creates the file.
- Parsing is flexible (rows may have differing column counts) and headerless — the first row is only bold-highlighted for readability, not treated specially on save.

## Keybindings

| Key | Action |
|---|---|
| `↑ ↓ ← →` | Move selection |
| `Alt+←` / `Alt+→` | Switch column page (when columns overflow terminal width) |
| `Enter` | Edit selected cell |
| `Enter` (while editing) | Confirm edit |
| `Esc` (while editing) | Cancel edit, revert cell |
| `Backspace` (while editing) | Delete last character |
| Double-click a cell | Edit that cell |
| Single click a cell | Select that cell |
| `Ctrl+S` | Save (prompts `y/n`) |
| `Ctrl+D` | Delete focused cell, shifting the column up (prompts `y/n`) |
| `Ctrl+Q` or `Q` | Quit — prompts to save if there are unsaved changes |
| `y` / `n` | Confirm / cancel a pending Save, Delete, or Quit prompt |

## Behavior notes

- **Live writes while editing**: keystrokes in edit mode are written into the in-memory cell immediately (not just on confirm), so `Esc` reverts to the value at the moment editing started.
- **Auto-expanding grid**: a trailing empty row and column are always kept, so you can extend the sheet by editing into the last row/column.
- **Cell delete = shift-up**: `Ctrl+D` removes the value by shifting all cells below it in the same column upward (spreadsheet-style delete, not full row/column removal).
- **Save trims blank tail**: on save, `rcsv` writes only up to the last row/column that contains data — trailing empty rows/columns from the auto-expand are not persisted.
- **Column paging**: if the row is wider than the terminal, columns are split into pages; the status bar shows `[Page X/Y]`.
