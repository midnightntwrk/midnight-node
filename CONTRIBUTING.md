# Contributing

We welcome your contributions to the Midnight network! By contributing, you'll play a vital role in shaping the future of a blockchain focused on data privacy.

## Getting Started

* **Review Existing Contributions and Issues:** Before submitting, please check if a similar issue or feature request already exists by searching our issue tracker.
* **Understand the Project:** Familiarize yourself with Midnight's architecture, technology, and coding standards. You can find relevant information in our litepaper. 
* **Set up Your Development Environment:** Ensure you have the necessary tools and dependencies installed. See our developer [documentation](https://docs.midnight.network/) for detailed instructions. 

## Submitting Issues

Use one of the [templates] to submit an issue to the Project Board. The Midnight team or a community member will address it if it's relevant.
Ensure the title is a clear summary of the requirement and provides enough context.

**Issue Types:**

* **Bug Report:** Provide detailed information about the issue, including steps to reproduce it, expected behavior, and actual behavior, screenshots, or any other relevant information.
* **Documentation Improvement:** Clearly describe the improvement requested for existing content and/or raise missing areas of documentation and provide details for what should be included.  
* **Feature Request:** Clearly describe your feature, its benefits, and most importantly, the expected outcome. This helps us analyze the proposed solution and develop alternatives.
* **Enhancement:** (WIP)

## Developer Certificate of Origin (DCO)

All contributions must include a sign-off in every commit message, certifying that you have the right to submit the code under the project license. This is done by adding a `Signed-off-by` trailer using `git commit -s`:

```
git commit -s -m "feat: your commit message"
```

This produces a commit message like:

```
feat: your commit message

Signed-off-by: Your Name <your@email.com>
```

By signing off, you agree to the [Developer Certificate of Origin (version 1.1)](https://developercertificate.org/).

If you have forgotten to sign off past commits in a PR, you can amend them:

```bash
# Amend the last commit
git commit --amend -s --no-edit

# Or rebase to sign off multiple commits (replace N with the number of commits)
git rebase --signoff HEAD~N
```

A DCO GitHub App runs on every pull request and will block merges until all commits are signed off.

### Automating sign-off

To avoid having to remember `-s` on every commit, install a `prepare-commit-msg` hook in your clone of this repo that appends the sign-off automatically:

```bash
cat > .git/hooks/prepare-commit-msg <<'EOF'
#!/bin/sh
NAME=$(git config user.name)
EMAIL=$(git config user.email)
grep -qs "^Signed-off-by: " "$1" || printf "\nSigned-off-by: %s <%s>\n" "$NAME" "$EMAIL" >> "$1"
EOF
chmod +x .git/hooks/prepare-commit-msg
```

After installing the hook, every `git commit` in this repo will include a `Signed-off-by` trailer automatically. Make sure your `user.name` and `user.email` are set correctly, since the hook certifies the DCO on your behalf for every commit.

NOTE: DCO strictly forbids automatically signing off AI generated code. Please instruct any AIs that they are not allowed to use `-s` or sign off code on your behalf. (See https://github.com/torvalds/linux/blob/master/Documentation/process/coding-assistants.rst#signed-off-by-and-developer-certificate-of-origin )

## Code Contribution Process

* **Pull Requests:** Code contributions are submitted via Pull Requests.
* **Fork the Repository:** Create your own fork of the Midnight repository.
* **Create a Branch:** Make your changes in a separate branch,
  prefixed with a short name moniker (e.g. `jill-my-feature`).
* **Follow Coding Standards:** Adhere to the coding style guides specified in our documentation.
* **Write Tests:** Include unit tests and integration tests to cover your changes.
* **Commit Messages:** Write clear and concise commit messages, and always sign off with `git commit -s`.
* **Submit Pull Request:** Submit your pull request to the appropriate branch in the main repository.
* **Please do not `--force` pushes** - doing so means that reviewers will have to re-review all
  commits in the PR rather than commits since last review.
* **Code Review:** All pull requests undergo code review by project maintainers.
  Be prepared to address feedback from reviewers.

## Requirements for Acceptable Contributions:

* **Coding Standards:** Code must adhere to the coding style guides defined in our documentation
* **Testing:** New functionality must include corresponding unit tests and integration tests.
* **Documentation:** Code changes should be accompanied by proposed relevant documentation updates.
* **License:** All contributions must be compatible with the project's license.
  Where possible all files should have this license header:

```
// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
```

Where this is not possible, a copy of the Apache 2.0 or the repository's top-level LICENSE file in the same directory is required

## Support and Communication:

Ask anything about Midnight! We're here to help. Connect with us on [Discord](https://discord.com/invite/midnightnetwork), [Telegram](https://t.me/Midnight_Network_Official), and [X](https://x.com/MidnightNtwrk) and Join the Community to stay updated and engage with other Midnight enthusiasts.

We appreciate your contributions!
