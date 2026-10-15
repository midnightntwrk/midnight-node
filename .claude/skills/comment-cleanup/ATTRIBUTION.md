# Attribution

The idea for this skill came from a GitHub gist by
[bavanws](https://github.com/bavanws):
<https://gist.github.com/bavanws/123e0343f8a79cec825d9141124a0a83>

None of the gist's text is used here. `SKILL.md` is a clean-room rewrite:

1. [`specs/comment-cleanup.md`](https://github.com/ozgb/ozgb-skills/blob/fa07eb9/specs/comment-cleanup.md) describes the
   skill's behaviour in new words, with new examples.
2. A separate agent wrote `SKILL.md` from that spec alone. It had no access to
   the gist or to any earlier version of the skill.

Departures from the gist's approach:

- Section numbers in published, versioned standards (`RFC 9110 §8.3`) count as
  stable references; the gist treated all section numbers as unstable.
- Run the skill as a step before every push, scoped to the unpushed commits,
  rather than relying on it triggering on its own. This step was added here
  after the clean-room rewrite and is not in the upstream spec.
