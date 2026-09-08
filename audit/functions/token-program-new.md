## `Program::new` in programs/demo-token/app/src/lib.rs (L472-L484)

**Purpose:** Constructor. Sets metadata, makes the deployer the admin, and configures the faucet. Every later authorization check (L156, L160) and every faucet parameter (L140, L142, L145) originates here or in `set_faucet`.

**Inputs & Assumptions:**
- `name`, `symbol` (String), `decimals` (u8): stored verbatim (L475-L477). Trust: untrusted (deployer-chosen); no validation.
- `faucet_amount` (U256): base units (comment L472). Trust: untrusted; zero means disabled (L140).
- `faucet_cooldown_secs` (u64): stored as `saturating_mul(1000)` ms (L480). Trust: untrusted; zero and saturating values accepted (token-state-faucet-claim.md).
- Implicit: `admin = Syscall::message_source()` (L478) — the deploying account or program.
- Preconditions: none.

**Outputs & Effects:**
- `Program { state }` with `total_supply = 0`, empty `balances`, `allowances`, `minters`, `last_claim`, `paused = false` (`..Default::default()`, L481; `TokenState` derives `Default`, L28).

**Block-by-Block:**

```rust
// L474-L482
let state = TokenState {
    name, symbol, decimals,
    admin: Syscall::message_source(),
    faucet_amount,
    faucet_cooldown_ms: faucet_cooldown_secs.saturating_mul(1000),
    ..Default::default()
};
```
- **What:** initial state.
- **Why here:** the only place the invariant sum(balances) == total_supply is seeded (both zero).
- **Assumes:** the deployer address is one the operator controls; nothing establishes this beyond deployment itself.
- **Establishes:** `admin != 0` (a message source is never zero); `minters` empty, so at deployment only the admin can mint (L160); faucet parameters as given.
- **Depended on by:** `ensure_admin`, `ensure_minter`, `faucet_claim`, `Faucet::config`.

```rust
// L486-L497
pub fn vft(&self) -> Vft<'_> { ... }
pub fn admin(&self) -> Admin<'_> { ... }
pub fn faucet(&self) -> Faucet<'_> { ... }
```
- **What:** service constructors in declaration order.
- **Why here:** order determines route indices 1, 2, 3 (sails-macros-core-1.0.1/src/program/mod.rs L166-L181, L695-L700); `VFT_ROUTE_IDX = 1` (L22) and the vault's generated client (`ROUTE_ID_VFT = 1`, vault token_client.rs L7) both depend on `vft` staying first (comment L486).
- **Assumes:** nobody reorders the three methods.
- **Establishes:** route/service mapping.
- **Depended on by:** `emit_vft_transfer` and every client.

**Cross-Function Dependencies:**
- Callee `Syscall::message_source` (external-source-available, sails-rs-1.0.1/src/gstd/syscalls.rs L30-L32).
- Callers: the deployment message (tests/gtest.rs L25-L31).
- Shared state: seeds every field.
- Invariant couplings: `decimals` is informational only — no arithmetic in the file uses it.

**Open Questions:**
- None.
