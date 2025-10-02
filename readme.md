# RYE

Named after whiskey, made for me.

A cli tool to chat with LLM's and store's your conversations
in markdown files so they're always searchable.

Chats are stored at ~/.rye or you can specify the path
with the environment variable `RYE_CONVERSATIONS` set to
your preferred path.

You'll be able to bring a chat back and continue.

Each conversation will be a new markdown file, where the LLM won't
bog you down with information.
This arose because I was constantly switching from the claude app
back to the terminal and I had to take my fingers off of the
keyboard to go and copy whatever claude said and I also wanted
to have a history to look back at. LLM's generate a lot of information
much of which they repeat.

The LLM will be prompted to output in markdown and it will be appended
to the file, and instead of them repeating them selves they will just
refer to previous sections in the markdown file.

So far just vibe coded, how wonderful.
