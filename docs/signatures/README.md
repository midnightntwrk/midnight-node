This repo requires signed commits.

At some early point in the history of the project some commits were not signed.
Unfortunately signing them later would disrupt later commit signatures. The
author of those commits has kindly signed the affected patches and these have
been uploaded as git notes to the relevant commits (so not to disrupt the hashes).

As well as adding them as `git notes` they are included in this directory for posterity.

To see a git note one must first download the notes as they are not included with
clone by default:

```sh
git fetch origin refs/notes/commits:refs/notes/commits
git notes show 10d2ac1c
```
