# Project instructions

## Orientation

Use HiveX to recover relevant decisions, dependencies and exceptions before proposing or changing behavior. The [documentation map](docs/README.md) identifies the Markdown authorities; consult the parts relevant to the task.

## Development

Follow the [engineering guideline](docs/guidelines/engineering.md) for implementation and review. Use the project's documented setup and verification commands, scoped to the affected work.

For issues, triage and pull requests, follow the [tracker procedure](docs/procedures/issue-tracker.md). Preserve decisions and authorizations already supplied by the owner.

<!-- hivex:skills -->

## HiveX skills

Every task loads the HiveX skill that matches its work before starting, with no exception for small changes:

- Any change to what people see, a correction included, starts with `hivex-design`, its visual review criteria, the project's design guide and the platform's official interface guidelines for the parts involved; its evidence notes the guidance followed and any deliberate departure.
- Code uses `hivex-implement`; documentation `hivex-document`; reviews `hivex-review`; tracker, Git, pull requests, CI and merges `hivex-git`; and every task `hivex` to recover its sources.
- Instructions for a particular task may add skills, never remove one. Delegated work follows this section, and its instructions refer to it instead of restating the skills.

HiveX maintains this section: `hivex update` replaces it with the installed release's.

<!-- /hivex:skills -->
