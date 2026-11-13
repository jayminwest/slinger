---
name: slinger-listen
description: Take the slings the user sends from a slinger canvas in this session. Waits for each sling with `slinger session listen`, answers it on the canvas, then waits for the next. Use when the user says "listen to slinger", "take my slings", "/slinger-listen", or names a workspace to listen to.
---

# slinger-listen

The user slings from a canvas: a selection, the new ink and a prompt. While
you listen, each sling of a workspace comes to this session in place of a
process that slinger starts. You answer with your own tools and permissions.

1. Find the daemon. Run `slinger workspace list`. It finds the running
   daemon on any socket.
2. Find the workspace. If the user did not name one, take the one in the
   list whose directory holds the current directory. Ask only when none or
   more than one fits.
3. Take over from the hidden process. Run `slinger session show
   <WORKSPACE>`. With `process running`, slinger started a process for this
   agent that the user cannot see, and a sling can go to it while you do not
   listen. Stop it with `slinger session stop <WORKSPACE>`; do not ask. Only
   when its `status` is `working`, tell the user first: the stop ends its
   turn.
4. Wait for a sling. Run this in the background, so the user can still talk
   to you, and act when it exits:

   ```bash
   slinger session listen <WORKSPACE> --pid $PPID
   ```

   `$PPID` is this session's process: if it ends while you work on a sling,
   slinger knows you are gone and keeps the sling for the next listener. It
   prints one sling and exits. The first line names your agent
   (`agent=<NAME>`); the second names the directory of the packet's images
   (`at=<DIR>`).
5. Read the images you need: each `img=` of the packet is a PNG in `at=`. The
   lines give only the place of the ink, the images give the words.
6. Do what the prompt asks. Answer on the canvas, not in chat: `slinger
   canvas put <CANVAS> --beside <ITEM> --text "..." --agent <NAME>`. Keep
   each text short. Run `slinger guide` for the other verbs.
7. Say that you are done: `slinger status set done --agent <NAME>`, or
   `slinger status set blocked --note "what you need" --agent <NAME>`. Either
   ends your work on the sling.
8. Go to step 4 at once, before you write anything to the user. Listen
   until the user tells you to stop.

Rules:

- A sling waits while you work: you get it when you listen again. Do not run
  two listeners for one workspace.
- To stop, end the background command. For 10 seconds after that slinger
  still counts you as listening: a sling sent then waits for the next
  listener. A sling sent later starts the agent's process, resuming its
  conversation, and goes to it.
- Exit code 6 means the daemon is not running. Tell the user; do not start
  it unless the user asks.
- Change only the items you made. Never change the user's ink.
