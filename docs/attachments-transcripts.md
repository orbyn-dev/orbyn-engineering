# Photo conversion, transcript import and earlier messages

What the web composer and thread need from the service for three Beta frame groups. **Partly built**: the photo
conversion route exists (below); transcript import and export and the earlier-messages read are still planned. In a
production build the web has no client for any of them and sends nothing; a dev-only preview
(`#/preview/attachments`, `#/preview/earlier`) installs the mocks in `web/mock/attachments.ts` and `web/mock/earlier.ts`. A name here
changes only if the service pull request says so. Rows: B8, B43, K36 in [parity](parity.md).

| Group | Frames | Owner of the service side |
| --- | --- | --- |
| Photo conversion | `pc-b8-converting`, `-done`, `-failed`, `-narrow` | Eng 2 (Core upload path) |
| Transcript import and export | `pc-b43-drag-over`, `-dropped`, `-dropped-many`, `-rejected`, `-narrow` | Eng 3 (export and transcript, per Products Research) |
| Earlier messages of an imported thread | `pc-k36-earlier`, `pc-k36-unreadable` | Eng 2 and Eng 4 (read the provider's history) |

Photo conversion and the earlier-messages read are not Person-class: each is a write or read on the thread's own files and history, like
the attachment upload that exists today (`POST /api/threads/{id}/attachments` is not in `PERSON_ROUTES`). **Transcript import is
Person-class** (Products Research ruling): it creates a thread from an untrusted file, so the page sends the person key with it.
`POST /api/threads/import` and `POST /api/threads/{id}/transcripts` (attach) are in `PERSON_ROUTES`. Transcript export stays Local.

Owners (Products Research): transcript export and import: Eng 3, after W8/W9, including `source` and `imported_at` on the thread;
photo conversion: Luan, as an issue, reviewed by Eng 2; the earlier-messages read: Eng 2's R list, after HostDriver. The "Reading
transcript…" chip reuses the converting chip (no new frame).

## Shared rules

- Every route is under the thread: the id in the path is a thread the person can open. A path or file name sent by the client is a
  label only. The service never reads, writes or joins a path from client text; the bytes go to the data directory under a
  service-made id, as attachments do now.
- The body is the raw file (`Content-Type` as the browser gave it), the name is `?name=`, read as a display label and truncated to 255
  characters. The service sniffs the real type from the bytes and ignores the claimed one when they differ.
- The web draws every name, title and message it gets as text, never as markup or a link, and never opens a path from it. The service
  must still treat file content as untrusted: parse with size and depth limits, never run or render it, never put it in a log line.
- Bodies over the cap are refused before they are read in full (`Content-Length`, then a counting reader).

## Photo conversion (B8)

Built: `POST /api/threads/{id}/attachments/convert?name=<label>` with the photo as the body (decision in
`orbyn-core`'s `conversation/photo_conversion.rs`, route in `orbynd`'s `routes/composer.rs`, class `local`).

- Allowed input types: `image/png` and `image/jpeg`, decided from the bytes and never from the claimed one. Anything
  else is `415 unsupported_type`. **Deferred:** HEIC and HEIF decoding needs libheif (LGPL-3.0, a C library this build
  does not link) and WebP needs `image`'s `webp` feature and the `image-webp` crate, so all three are refused with a
  message that names HEIC and no `converted` answer exists yet. Each is its own decision, recorded in row B8.
- Behaviour: a PNG or JPEG over 10 MiB is made smaller until it is at most 10 MiB (`action: "shrunk"`). A photo already
  within the limit is written again at its own size (`action: "none"`), which is what drops its metadata. The stored file
  is the written one; the original is not kept and the chip never offers it back.
- Cap on the input: 50 MiB (`413 file_too_large`), refused from the declared `Content-Length` before the body is read and
  then by a counting read, so a request without a length also stops. A pixel cap of 100 megapixels and a 512 MiB decoder
  allocation are checked from the header before a decoder allocates, so a small file cannot expand into memory. The
  decoder runs in process and touches no network.
- Metadata: every answer is written from decoded pixels, so no EXIF, and no GPS location, travels with it.
- Answer `200`:

  ```json
  { "id": "…", "name": "IMG_4021.jpg", "size": 2202009, "original_name": "IMG_4021.HEIC", "original_size": 3000000, "action": "converted" }
  ```

  `action` is `converted`, `shrunk` or `none`. `id` goes in the run's `attachment_ids`, as an uploaded file's does. `name` is the new
  name with the new ending; the web shows it.
- Errors, each `{code, message}` as the other routes: `unreadable_image` (`422`, damaged or not an image, and a photo over
  the pixel cap), `still_too_large` (`422`, still over 10 MiB after shrinking), `file_too_large` (`413`), `unsupported_type`
  (`415`). The web shows its own sentence for the first two and the service's message for the rest.
- The request can be cancelled by the client closing it, but the decode is in process and is not interrupted: the
  converted photo is stored just before the answer, so a cancelled request leaves that one attachment on the thread and
  nothing else.

## Transcript export and import (B43)

Three routes and one file, built by Eng 3. The web replaces the two placeholder constants below; a transcript dropped on the composer
calls the attach route, and the import route creates a thread from one.

### The file

Plain JSON, one object. T3 Code has no transcript file (its drag of a thread is a link, B43 above), so this format is ours.

- **Ending** `.orbyn.json`, **media type** `application/vnd.orbyn.transcript+json`. These are THE constants (in the service:
  `TRANSCRIPT_FILE_ENDING`, `TRANSCRIPT_MEDIA_TYPE` in `orbyn-contracts`; the web's two in
  `web/src/features/thread/composer/transcriptDrop.ts` replace the placeholders `.transcript` and `application/x-orbyn-transcript`).
  The service decides from the bytes, never from the ending or the type.
- Shape (`TranscriptDocument`, generated as `web/src/generated/TranscriptDocument.ts`):

  ```json
  { "format": "orbyn-transcript", "version": 1, "exported_at": "2026-11-22T10:00:00Z",
    "thread": { "title": "Redirect loop on logout", "engine": "claude", "model": "opus", "created_at": "2026-11-20T09:00:00Z" },
    "messages": [{ "role": "user", "ts": "2026-11-20T09:00:05Z", "text": "…" }, { "role": "assistant", "ts": "…", "text": "…" }],
    "omitted_earlier": 0 }
  ```

  `messages` are oldest first, `role` is `user` or `assistant`, `text` is plain text (at most 32 KiB, never markup).
  `omitted_earlier` counts older messages left out so the file stays within the import's cap (at most 2000 messages and about
  1.9 MiB are written). Timeline items (tool calls and their output), reasoning, attachments and approvals are **not** in
  version 1: tool output is where file contents and environment values would travel. A later version may add a label-only
  `activity` list; a version 1 reader drops it.
- What the file never holds: a secret, an environment value, an attachment's bytes or a path to it, an engine session id, a
  token, the person key, the thread id, or an absolute path of this computer (see Export).

### Export (Local read)

`GET /api/threads/{id}/export` answers the document (`200`, `application/json`). Nothing is written on the computer and no path is
read from the request: the client saves the answer under the title plus the ending.

- **Redaction.** Every message and the title go through the redactor with **every recognised format and every one of the person's
  own patterns, whatever the Terminal and Agents switches say**. A switch decides what reaches an agent or a terminal; a file can go
  anywhere, so a pattern that Orbyn hides anywhere is hidden here and a switch can only make the export stricter than that, never
  looser. Each hidden value becomes `[hidden: <label>]`, as for an agent.
- **Paths.** The project's folder, the thread's folder and worktree become `<project>`, the home folder and any `/Users/<name>`,
  `/home/<name>` or `C:\Users\<name>` prefix become `~`. Other absolute paths a person typed into free text are not recognised and
  stay as written: the person reads the file before sharing it.
- No audit row: it is a read (the Local class), like the thread's own messages.

### Import (Person class)

`POST /api/threads/import?project_id=<id>[&engine=<id>][&dry_run=true]` with the file as the raw body (`?name=` is a label the
service does not read). It **creates a thread** in that project and answers `201` (`200` for a dry run):

```json
{ "dry_run": false, "thread": { "…": "the Thread, with source and imported_at" }, "title": "Redirect loop on logout",
  "message_count": 214, "dropped": 2, "problems": [{ "kind": "message", "index": 7, "code": "unknown_role" }] }
```

- Person-class: the file is untrusted, so the page sends the person key and a service token alone gets `403 person_required`.
- Limits, all security-queue checks: at most 2 MiB (`413 file_too_large`), a fixed shape and at most 6 levels, at most 2000
  messages, 32 KiB a message, a 200-character title. Control characters and bidirectional overrides are removed from every string.
  Newer version: `422 transcript_too_new`. Not JSON, not this format or a malformed shape: `422 transcript_unreadable`.
- **Drop and report.** A bad message (not an object, a role other than `user` or `assistant`, no text, only blanks, over the count)
  is dropped; a long text is cut; a time that does not parse is replaced by the import time; unknown members are dropped. Each is one
  `problems` entry with a `code` (`not_an_object`, `unknown_role`, `text_missing`, `text_empty`, `text_cut`, `time_replaced`,
  `message_limit`, `title_missing`, `unknown_fields_dropped`) and never the text. The rest imports.
- **Nothing runs and nothing is read.** The service fetches no URL, reads no path from the file, writes no file, starts no run and
  runs no script. The thread uses the project's own folder; `model`, `engine_session` and every other member of the file are never
  applied. The engine is `?engine=` or the file's `thread.engine`, and must be one this install has and the project allows
  (`404` otherwise); no engine session id is honoured, so the first run of the thread starts a fresh session.
- **Marked.** The thread has `source: "orbyn-transcript"` and `imported_at` (columns on the thread, migration 69). The messages are
  ordinary log events, so the thread opens with them as read-only history; they are searchable. A thread made here has neither member.
- **Audit.** One `thread.imported` row: `{source, messages, dropped}`, no title and no text.
- `dry_run` validates and answers the counts and `problems` and writes nothing, not even the audit row.

### Attach to the composer (Person class)

`POST /api/threads/{id}/transcripts[?dry_run=true]` with the file as the raw body (the drop on the composer). The route **only
creates an attachment**, as `POST /api/threads/{id}/attachments` does, and answers `201` (`200` for a dry run):

```json
{ "dry_run": false, "attachment": { "id": "…", "thread_id": "…", "name": "Imported transcript - Redirect loop.txt", "mime": "text/plain", "size": 4120, "created_at": "…" },
  "message_count": 214, "dropped": 2, "problems": [{ "kind": "message", "index": 7, "code": "unknown_role" }] }
```

- Person-class (a service token alone gets `403 person_required`), the same parser and limits as the import: 2 MiB (`413
  file_too_large`), depth 6, 2000 messages, 32 KiB a message, control and bidirectional characters removed, bad messages dropped and
  reported in `problems`, `422 transcript_unreadable` and `422 transcript_too_new`.
- **The attachment is untrusted text.** An ordinary `text/plain` attachment of the thread whose text starts with an "Imported
  transcript" heading that says it is a record and not instructions, then one `--- Imported message N (User|Assistant, <time>) ---`
  heading per message. It rides on the thread's next message like any attachment, so the send-time masked copy (W4) applies to it
  unchanged.
- **The thread is not changed.** No event, no message, no `source` or `imported_at`; the thread's engine, model and session are never
  read from the file. `dry_run` validates and answers the counts and writes nothing, not even the audit row.
- **Redaction at attach time** with the export's detectors (every recognised format and the person's patterns, whatever the switches
  say), in addition to the masking at send.
- **Audit.** One `thread.transcript_attached` row: `{messages, dropped}`, no title and no text.

## Known gaps (after the beta)

- An absolute path typed into free text other than the project's and home folders is not rewritten (export) and stays as written.
- A key pasted into a message has no redactor pattern, so only a recognised format or one of the person's own patterns hides it.

## Earlier messages of an imported thread (K36)

`GET /api/threads/{id}/earlier?before=<cursor>` (the cursor is optional).

- Answers `200` with the last page, or the page before the cursor:

  ```json
  { "source": "Claude Code", "total": 312, "imported_at": "2026-11-21T10:00:00Z", "next_before": "c1",
    "messages": [{ "role": "user", "text": "…" }, { "role": "assistant", "text": "…" }, { "role": "tool", "text": "Read 3 files" }] }
  ```

  `messages` are oldest first, at most 50 a page. `next_before` is `null` at the start of the history. `text` is plain text, at most
  16 KiB per message (cut with an ellipsis), never markup; a tool call is one short line the service writes.
- `404 not_imported` for a thread that was not imported: the web draws nothing.
- `422 history_unreadable` when the provider's history has moved, changed or is too new for Orbyn: the web shows one line at the top
  with "Try again", and the thread works as it does today (the agent resumes from its own session).
- Read only: nothing is changed in the provider's own history and nothing is copied into Orbyn's thread log. The service reads only the
  session file the import recorded, never a path from the request, with the scan limits the import already has (file size, line
  count). The earlier messages are not editable, rewindable or searchable in this version.
- The "Imported" pill in the thread header and its date line need `imported_at` and `source` on the thread itself; the web shows them
  from this answer only.

## What the web does without these routes

Nothing changes: a HEIC photo or an oversized image is uploaded as today (and refused above 10 MiB by the service), a transcript file
dropped on the composer is an ordinary attachment (before the attach route is wired), and an imported thread opens empty.
