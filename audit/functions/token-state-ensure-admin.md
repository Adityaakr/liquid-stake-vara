## `TokenState::ensure_admin` in programs/demo-token/app/src/lib.rs (L155-L157)

**Purpose:** The admin gate for role and configuration changes: `grant_minter` L319, `revoke_minter` L331, `set_faucet` L344, `transfer_admin` L357, `pause` L370, `resume` L382.

**Inputs & Assumptions:**
- `who` (ActorId): always `message_source()` at every call site (L316, L328, L341, L354, L367, L379). Trust: trusted identity.
- Implicit: `admin` (L156).

**Outputs & Effects:**
- `Ok(())` iff `admin == who` (L156); otherwise `Err(Unauthorized)`. No writes.

**Block-by-Block:**

```rust
// L156
if self.admin == who { Ok(()) } else { Err(TokenError::Unauthorized) }
```
- **What:** equality on one stored address.
- **Why here:** first statement inside every admin handler's borrow block, before any write.
- **Assumes:** `admin` is non-zero and controlled. Established by: `Program::new` sets it to the deployer (L478); `transfer_admin` refuses zero (L358). Nothing else writes `admin`.
- **Establishes:** caller is the single admin.
- **Depended on by:** all six callers.

**Cross-Function Dependencies:**
- No callees. Callers listed above. Shared state: `admin` written by `Program::new` L478 and `transfer_admin` L359.
- Invariant couplings: exactly one admin at a time; there is no pending/accept step, so `transfer_admin` immediately moves this gate to `to`.

**Open Questions:**
- None.
