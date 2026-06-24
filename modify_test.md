# modify_test

## Scope
- `examples/apps/new-input-form/src/agent.rs`
- `examples/apps/new-input-form/src/app.rs`

## Why
- `ChoicePrompt` was added to the turn state machine and needed compile-time coverage for all `match` arms.
- `App` tests were still asserting against the private `pending_choice_group` field.

## Test queue
1. `app::tests::tick_active_turn_auto_opens_choice_group_from_assistant_payload`

## Verification
- `cargo check -p new-input-form`
- Single-case `cargo test` for the impacted unit test above
