# Folder project revisions

`folderProject(handle)` uses SHA-256 revisions of file bytes. Each `createFileSession()` tracks its own successful reads and writes. `readOnly()` supports inspection and unused-asset review without advancing an editor's baseline.

Writing an unread path creates a new file only. Existing files require a matching read baseline; removal also requires a prior read. Binary asset writes and deletion use explicit revisions. Sessions created from one adapter share per-path mutation queues, so concurrent saves with the same baseline cannot both succeed. New custom-item composites keep the same create-only behavior.

File System Access does **not** offer atomic compare-and-swap against other applications. The adapter checks bytes before opening a writable stream and again before closing its staged write, but an external application or independently created adapter can still change the file between the final check and commit. These guards detect observed conflicts; they do not claim the stronger atomic revision guarantee provided by the SDK server. Reopen a conflicted file before saving. Closing or failing a write never advances the saved baseline unless the write succeeds.
