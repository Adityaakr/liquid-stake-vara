## `TokenState::ensure_live` in programs/demo-token/app/src/lib.rs (L79-L81)

**Purpose:** The single pause gate. Every balance- or allowance-changing state transition (`transfer` L98, `approve` L106, `mint` L121, `burn` L131, `faucet_claim` L139) starts by calling it; without it `paused` (L38) would have no effect anywhere.

**Inputs & Assumptions:**
- `&self`: reads `self.paused` (L80). Trust: trusted (program-private state).
- Implicit: none. No caller identity, no clock.
- Preconditions: none.

**Outputs & Effects:**
- Returns `Err(TokenError::Paused)` when `paused == true`, `Ok(())` otherwise (L80). No state writes, no events.

**Block-by-Block:**

```rust
// L79-L81
fn ensure_live(&self) -> Result<(), TokenError> {
    if self.paused { Err(TokenError::Paused) } else { Ok(()) }
}
```
- **What:** Reads one bool and maps it to a Result.
- **Why here:** Called first in every mutating transition so no write happens before the pause check.
- **Assumes:** `paused` is only written by `Admin::pause` (L371) and `Admin::resume` (L383). Established by: grep of the file — no other writer; the field is `pub` (L38) but `TokenState` is only reachable through `Program.state` (L467), which is private.
- **Establishes:** "the token is not paused" for the remainder of the calling transition.
- **Depended on by:** all five callers listed above, and transitively `Vft::*`, `Admin::mint`, `Admin::burn`, `Faucet::claim`.

**Cross-Function Dependencies:**
- No callees.
- Callers: `transfer` L98, `approve` L106, `mint` L121, `burn` L131, `faucet_claim` L139. Not called by `spend_allowance` (L112-L118), nor by any Admin role/config operation (`grant_minter` L315, `revoke_minter` L327, `set_faucet` L340, `transfer_admin` L353, `pause` L366, `resume` L378), nor by any query.
- Shared state: `paused` written by `Admin::pause`/`Admin::resume`.
- Invariant couplings: pause is global; there is no per-operation pause. Role and configuration changes stay possible while paused.

**Open Questions:**
- None.
