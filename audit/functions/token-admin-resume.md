## `Admin::resume` in programs/demo-token/app/src/lib.rs (L377-L387)

**Purpose:** Exported `Resume() -> bool throws TokenError` (IDL L89). Clears `paused`.

**Inputs & Assumptions:**
- No payload.
- Implicit: `caller = message_source()` (L379); `admin`.
- Preconditions: caller is admin — `ensure_admin` L382. Callable while not paused (idempotent, still emits).

**Outputs & Effects:**
- Success: `paused = false` (L383); `Resumed { by: caller }` (L385); reply `true` (L386).
- Error: `Unauthorized` (L382), structured panic (L377).

**Block-by-Block:**

```rust
// L382-L383
s.ensure_admin(caller)?;
s.paused = false;
```
- **What:** authorize, clear flag.
- **Why here:** single write.
- **Assumes:** nothing.
- **Establishes:** `ensure_live` succeeds from the next message; allowances and balances are exactly as they were at pause time (no transition touches them while paused, token-state-ensure-live.md).
- **Depended on by:** all mutating VFT/Admin/Faucet paths.

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event`.
- Callers: admin only (tests/gtest.rs L130).
- Shared state: `paused` (also `pause` L371).
- Invariant couplings: none beyond the pause flag.

**Open Questions:**
- None.
