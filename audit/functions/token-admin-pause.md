## `Admin::pause` in programs/demo-token/app/src/lib.rs (L365-L375)

**Purpose:** Exported `Pause() -> bool throws TokenError` (IDL L88). Sets the global `paused` flag that `ensure_live` reads.

**Inputs & Assumptions:**
- No payload.
- Implicit: `caller = message_source()` (L367); `admin`.
- Preconditions: caller is admin — `ensure_admin` L370. Callable while already paused (idempotent write, still emits).

**Outputs & Effects:**
- Success: `paused = true` (L371); `Paused { by: caller }` (L373); reply `true` (L374).
- Error: `Unauthorized` (L370), structured panic (L365).

**Block-by-Block:**

```rust
// L370-L371
s.ensure_admin(caller)?;
s.paused = true;
```
- **What:** authorize, set flag.
- **Why here:** single write.
- **Assumes:** nothing.
- **Establishes:** from the next message, `transfer` (L98), `approve` (L106), `mint` (L121), `burn` (L131) and `faucet_claim` (L139) return `Paused`. Role and faucet configuration, `resume`, `transfer_admin` and all queries remain available.
- **Depended on by:** the vault indirectly: its `deposit` (vault L747), `redeem` (L772), the unbond claim push (L810), `fund_rewards` (L834) and the push at vault L907 all reach the token and receive `Ok(Err(Paused))` (vault L719/L729). The vault has no pause-aware branch beyond mapping this to `TokenRejected`.

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event`.
- Callers: admin only (tests/gtest.rs L126-L129).
- Shared state: `paused` (also `resume` L383).
- Invariant couplings: the flag has no expiry and no per-operation granularity.

**Open Questions:**
- None.
