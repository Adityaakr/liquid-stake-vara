## `Pool::fund_rewards` in programs/vara-pool/app/src/lib.rs (L760-L776)

**Purpose:** Anyone adds the attached VARA to the vesting tranche (comment L758-L759). Refunds on error.

**Inputs & Assumptions:**
- No parameters. Implicit: `message_value` (L762), `message_source` (L763, event only), `block_timestamp` (L767).
- Trust: untrusted sender; no `actor_for`, no `ensure_admin`, no `ensure_live`.
- Preconditions: none beyond `PoolState::fund`'s (`assets > 0`).

**Outputs & Effects:**
- On `Ok`: `PoolState::fund` writes persist; emits `RewardsFunded {from, assets, reserve, vesting_ends_at}` (L771) using the post-state `reserve` and `vest_end_ms` captured at L767; reply `Ok(reserve)` without value.
- On `Err` (`ZeroAmount` for a value-less call, or `Overflow`): reply `Err(e)` with the value returned (L774).

**Block-by-Block:**

```rust
// L762-L768
let value = Syscall::message_value();
let from = Syscall::message_source();
let assets = U256::from(value);
let outcome = { let mut s = self.state.borrow_mut(); s.fund(assets, now_ms()).map(|_| (s.reserve, s.vest_end_ms)) };
```
- **What:** Fund and capture the reported figures under one borrow.

```rust
// L769-L775
match outcome { Ok((reserve, vesting_ends_at)) => { emit ...; CommandReply::new(Ok(reserve)) } Err(e) => CommandReply::new(Err(e)).with_value(value) }
```
- **What:** Same reply/refund shape as `stake`.
- **Establishes:** any actor can, by attaching VARA, change `vest_end_ms` for everyone (weighted average in `fund` L401-L407): a large top-up stretches the release of the currently locked remainder toward a full vesting period; funding also works while paused.

**Cross-Function Dependencies:**
- Callee `PoolState::fund` (internal): see record; the handler relies on it having no writes on its `ZeroAmount` path (true, L397) so the refund leaves state untouched.
- Callee `Syscall::*`, `emit_event`: as in `Pool::stake`.
- Callers: dispatcher only.
- Shared state: vest fields, `reserve`.
- Invariant couplings: the only value entry point besides `stake`; both add exactly `message_value` to `reserve`.

**Open Questions:**
- unclear; whether permissionless funding is intended given its effect on `vest_end_ms` (comment L759 says anyone may fund).
