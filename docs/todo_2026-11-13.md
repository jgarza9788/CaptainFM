# TODO

## 1. Multi-select: make it work like most apps (tested 2026-10-29, fixed 2026-11-04)
The standard protocol (Explorer, Nautilus, Dolphin, Finder): a plain click or arrow selects only that item; Ctrl+click toggles; Shift+click and Shift+arrows select a range from a fixed anchor; Ctrl+arrows move the focus without selecting; Ctrl+A selects all; Esc or a click on empty space clears; right-click on an unselected item selects only it; Delete acts on what is highlighted and the focus moves to the neighbour. Tested on v0.0.3 (Dev2, binary of c165d78), default `fp` keymap, headless: select with keys, press Delete (or copy and paste), then read which files were acted on, plus screenshots. Vim keymap checked too. **The mouse cannot be driven headlessly; the mouse items are read from `qml/Pane.qml` and need a live check with the uinput pointer (scripts/uinput_pointer.py).**

What already matches: Shift+Up/Down extend from an anchor and shrink when reversed, Shift+Home/End, Ctrl+A, plain arrow after a Shift-selection collapses it, copy/cut/paste of a multi-selection, trash with Ctrl+Z restoring every item, selection kept across sort and hidden toggle, tree, columns and Miller views, drag carries the whole selection when started on a selected row, and the Vim keymap (`v` anchor and shrink, `m`, `v` toggle).

**Done 2026-11-04** (Dev2): every item below is fixed, and all but the rubber band are covered by `scripts/selection-tests.sh` (36 cases, headless: keys, plus mouse clicks fed through the same `Pane` functions the mouse handlers call, `!click:<row>[:C|S|CS]`, `!rclickrow:<row>`, `!clickempty`). Decisions: Ctrl+click and Ctrl+arrows count the item under the cursor as selected (a click or arrow selected it), so Ctrl+clicking a second item leaves both; Ctrl+Shift+arrows add the items passed over (they do not shrink); invert is Ctrl+Shift+A (Ctrl+I stays the preview pane), select by pattern Ctrl+S (Vim `*` and `+`); Ctrl+Left/Right keep seeking media in the list and move the focus sideways in grid views; Enter on more than 10 items asks first. **Still open: the rubber band and the live mouse pass.**

### Wrong, and can hit the wrong files (do first)
- [x] **A plain click does not clear the selection.** `Pane.qml` plain click only calls `setCursor`; `clearSelection` runs only in Shift-visual mode. After Ctrl+clicking two files, a plain click on a third leaves the two selected, and Delete / Ctrl+C / Ctrl+X act on the two, not the one just clicked (`targets()` prefers the selection). Fix: a plain click selects only that row (clear, then set cursor and anchor).
- [x] **Right-click on an unselected item acts on the old selection.** `rowRightClicked()` uses the selection when `selectedCount > 1`, so with two files selected, right-clicking a third opens its menu but the menu and the armed "Right-click again to delete 2 items" are for the other two. Fix: right-click on an unselected row clears the selection and selects that row first (both list and grid delegates).
- [x] **Plain arrows do not clear Ctrl+marks.** Only a Shift-extended selection is dropped. After Ctrl+A then Down, all 12 stay selected; after two Ctrl+Space marks, Down keeps them and Delete trashes the marks, not the item under the cursor. Same for the grid, typing a letter (type-ahead) and plain Home/End. Fix: every plain cursor move clears the selection, as with Shift-selections today. (The Vim keymap keeps `m` marks on purpose; leave that one.)
- [x] **Plain End, PageDown, PageUp and Home extend a Shift-selection** instead of collapsing it (S-Down x2 then End selected everything to the end). `moveBy` guards the arrows only; guard the others the same way.

### Missing keys
- [x] Shift+PageUp / Shift+PageDown do nothing (selection and cursor unchanged).
- [x] Ctrl+Up/Down/Left/Right (move the focus without changing the selection) and Ctrl+Shift+arrows are unbound. Then Ctrl+Space should toggle the focused item **without moving down**; today `sel.toggle` marks and moves down (right for `m` in Vim, odd for Ctrl+Space and Insert in the default keymap).
- [x] No invert selection (Ctrl+Shift+A / Ctrl+I) and no select by pattern.
- [x] Grid view: Shift+Left/Right do nothing, and Shift+Down moves one item, not one row (selected f02, f03, f04 in a row). Use `moveVertical` / `moveHorizontal` for the Shift variants as the plain arrows do.
- [x] Esc after a Shift-selection only leaves select mode and keeps the items selected; it takes a second Esc to clear (after Ctrl+A one Esc clears). Make the first Esc clear.
- [x] Enter on a multi-selection opens only the item under the cursor. Standard: open every selected file (and each selected folder in its own tab).

### Mouse (read from the code, not run)
- [x] Shift+click takes its anchor from the cursor, so it moves with every click, and it adds to the selection instead of replacing it: click row 2, Shift+click 6, Shift+click 4 should leave 2 to 4, and leaves 2 to 6. Keep a fixed `anchorRow` set by plain click and Ctrl+click; Shift+click clears, then selects anchor to row. Ctrl+Shift+click adds the range.
- [x] A click on empty space does not clear the selection (nothing handles it).
- [ ] No rubber-band (drag on empty space) selection; Ctrl+drag should add. In list view the empty space is the area below the last row and the gaps; in grid view the gaps between cells. **Written (`BandArea` in `Pane.qml`, scrolls at the edges) but never run: a drag cannot be sent headlessly. Check it in the live pass.**
- [x] In Shift-select mode a plain click behaves like a Shift+click (`setCursor` extends while `visualMode` is on).

### Feedback and aftermath
- [x] After Delete, the cursor jumps to the first row (Down x2, Delete, Delete trashed d1 then f01, not f01 then f02; End then Delete moved the cursor to the top, not f09). Standard: the cursor goes to the item that took the place of the deleted one, or the one before it at the end of the list. After a multi-delete, the item after the last deleted one.
- [x] The count and the action disagree under a filter: Ctrl+A then a filter that shows 1 item says "12 selected · 1 item", but Delete acts on the 1 visible item. Either count only what is shown, or drop the hidden selection when the filter is typed.
- [x] In list view a selected row has only a yellow name (grid cells also get a tint). Add a background tint to selected rows so a selection is readable at a glance; keep the stronger cursor highlight.
- [x] Ctrl+C / Ctrl+X clear the selection at once; Explorer and Nautilus keep it. Low priority; a toast already says what was copied.

### After the fixes
- [x] Add the checks as `CFM_KEYS` scenarios (the Delete-and-list oracle used here: `<keys> <Del> !accept`, then list the scratch trash) in `run-tests.sh` or a script next to `scripts/screenshots.sh`, so the protocol cannot regress. The harness used for this pass is not in the repo.
- [ ] Re-test the mouse items live, in list, grid, tree and gallery views, with a Ctrl+click, Shift+click, empty-space click and rubber-band pass (uinput pointer, moves the real cursor). Also drag a tab out (section 2) and wheel the gallery strip.

## 2. Draggable tabs
- [x] Drop a file onto a tab to put it in that tab's folder (17.7).
- [x] Drag a tab out to make a split pane (2026-11-04). Drag a tab down out of the strip: an outline shows the side (the page edge nearest the pointer); on release its panes join the tab on show (dragging the tab on show uses its neighbour), at most 4 panes. Tested headlessly with `!tear:<tab>:<side>`; the drag gesture itself waits for the live pass in section 1.

## 3. Gallery view
A view mode with a large preview of the focused item and a scrolling filmstrip of thumbnails below it (like Finder's Gallery). Different from the grid (icons) view, which is only thumbnails. **Done 2026-11-04**: `Alt-7` / `, v G` (`, v g` was taken by icons), strip toggle `, v S`; the preview is an `Inspector` (its header is the info bar), and the side preview pane steps aside while the gallery is on. Screenshot `docs/images/gallery.png`; two gallery cases in `scripts/selection-tests.sh`.
- [x] New `viewMode: "gallery"` in `Pane.qml` (next to list / tree / miller / grid / columns), with `view.gallery` in `Commands.qml` (toggle back to list, as the other views do), an `Alt-` key and a `, v g` leader key. Add it to the Settings default-view choices and the README / HOWTO view lists.
- [x] Big preview area reuses the existing preview pipeline (`src/preview.rs`, `preview_extra.rs`, the Inspector's preview component) so images, video, code and PDF all work; folders and unknown types show a large icon.
- [x] Filmstrip reuses the grid's thumbnails (`src/thumbnails.rs`); the focused cell is highlighted and kept centred while scrolling.
- [x] Keys: Left/Right move through the strip (and update the big preview), Home/End, type-ahead, Enter opens, Space plays/pauses video; selection keys follow the multi-select protocol in section 1. Vim keymap: `h` / `l`.
- [x] Mouse: click a thumbnail to focus it, double-click to open, wheel scrolls the strip. (The wheel is not run headlessly; see the live pass.)
- [x] Optional: an info bar under the preview (name, size, dimensions, date) and a toggle to hide the strip.
- [x] Add a `CFM_KEYS` scenario and a screenshot to `scripts/screenshots.sh`.
