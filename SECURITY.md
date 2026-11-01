# Security policy

## Supported versions

Pearl is in early development (0.x) and has no release yet. Security fixes go into `master`; once
releases exist, only the latest is supported.

## Reporting a vulnerability

Report it **privately**: on GitHub, open the repository's **Security** tab and choose **Report a
vulnerability** (a GitHub Security Advisory). Please don't open a public issue.

Include what you can: the commit or version, the platform, the input or the steps that trigger it,
and what an attacker gains.

## What counts

Pearl opens documents people receive from others and talks to other people's machines:

- a document or project file that crashes the program, hangs it, exhausts memory, or reads or
  writes outside the document's folder;
- a session message that does any of that, reads what it shouldn't, or gets past the key in the link;
- anything that exposes the API key, a recovery file or a version to another user or machine;
- a document or a peer that gets Claude to do more than its tools allow.

A session link is a secret: whoever has it can join. Release binaries, when they come, are not
code-signed yet.
