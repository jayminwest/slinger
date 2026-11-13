---
name: slinger-listen
description: Take the slings the user sends from a slinger canvas in this session. Waits for each sling with `slinger session listen`, answers it on the canvas, then waits for the next. Use when the user says "listen to slinger", "take my slings", "/slinger-listen", or names a workspace to listen to.
---

# slinger-listen

The user slings from a canvas: a selection, the new ink and a prompt. While
you listen, each sling of a workspace comes to this session in place of a
process that slinger starts. You answer with your own tools and permissions.

1. Find the workspace. If the user did not name one, run `slinger workspace
   list` and take the one whose directory holds the current directory. Ask
   only when none or more than one fits.
2. Wait for a sling. Run this in the background, so the user can still talk
   to you, and act when it exits:

   ```bash
   slinger session listen <WORKSPACE> --pid $PPID
   ```

   `$PPID` is this session's process: if it ends while you work on a sling,
   slinger knows you are gone and keeps the sling for the next listener. It
   prints one sling and exits. The first line names your agent
   (`agent=<NAME>`); the second names the directory of the packet's images
   (`at=<DIR>`).
3. Read the images you need: each `img=` of the packet is a PNG in `at=`. The
   lines give only the place of the ink, the images give the words.
4. Do what the prompt asks. Answer on the canvas, not in chat: `slinger
   canvas put <CANVAS> --beside <ITEM> --text "..." --agent <NAME>`. Keep
   each text short. Run `slinger guide` for the other verbs.
5. Say that you are done: `slinger status set done --agent <NAME>`, or
   `slinger status set blocked --note "what you need" --agent <NAME>`. Either
   ends your work on the sling.
6. Go to step 2. Listen until the user tells you to stop.

Rules:

- A sling waits while you work: you get it when you listen again. Do not run
  two listeners for one workspace.
- To stop, end the background command. A sling sent after that waits for the
  next listener, or goes to the agent's process after `slinger session start
  <WORKSPACE>`.
- Exit code 6 means the daemon `slingerd` is not running. Tell the user; do
  not start it unless the user asks.
- Change only the items you made. Never change the user's ink.
