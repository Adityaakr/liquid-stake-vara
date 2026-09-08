## `ensure_gas` in programs/vault/app/src/lib.rs (L709-L711)

**Purpose:** Refuse to start an async flow unless the message still carries at least `required` gas, so the segment that runs after the token reply — the one that mints shares or compensates a failed payout — cannot run out of gas with funds already moved (comment L47-L50, L708).

**Inputs & Assumptions:**
- `required` (u64): always `GAS_ONE_CALL = 40_000_000_000` (L50) at every call site (L739, L762, L802, L827, L898). Trust: constant.
- Implicit: `Syscall::gas_available()` → `gcore::exec::gas_available()` (sails-rs-1.0.1 `src/gstd/syscalls.rs` L72-L74), the gas left in the current execution.
- Precondition: `GAS_ONE_CALL` covers pre-await work + `TOKEN_CALL_GAS` (6e9, L46, handed to the token via `with_gas_limit`) + re-instantiation and post-await work. Established only by the measurement note at L45-L50 ("~1.5B" for the call, "~17B" to instantiate again on a 1.10 node); nothing in code derives one from the other.

**Outputs & Effects:**
- Returns `Err(NotEnoughGas{required})` or `Ok(())`. No state writes, no messages.

**Block-by-Block:**

```rust
// L710
if Syscall::gas_available() < required { Err(VaultError::NotEnoughGas { required }) } else { Ok(()) }
```
- **What:** A single comparison at handler entry.
- **Why here:** First statement of every async handler, before any state read.
- **Assumes:** gas available at entry is a proxy for gas available after the wake-up. In Gear the reply-driven continuation runs on the gas reserved from the original message at the time of `wait_up_to` (gstd `locks.rs` L80-L82 → `exec::wait_up_to`), minus what the pre-await segment and the 6e9 sent with the token call consumed; nothing in this file measures the pre-await consumption.
- **Establishes:** a lower bound on gas at entry only.
- **Depended on by:** the correctness argument for post-await compensation (doc rule 2, L16-L17) — if the post-await segment traps on gas, that execution's writes are discarded but the pre-await writes (already persisted at the await) and the token's transfer stand.

**Cross-Function Dependencies:**
- Callee `Syscall::gas_available` (external, source available in sails-rs-1.0.1): a direct syscall wrapper; on non-wasm hosts it is a mockable value defaulting to 0 (`syscalls.rs` L215).
- Callers: `deposit`, `redeem`, `claim`, `fund_rewards`, `collect_fees`. Not `request_unbond` (synchronous, L787).
- Shared state: none.
- Invariant couplings: the 100-block default `wait_up_to` (gstd `config.rs` L73) applies to every token call; gas for the wait itself is charged by the runtime per block in the waitlist; nothing in this file accounts for it.

**Open Questions:**
- unclear; need to measure on the target runtime whether 40e9 at entry leaves enough after a 100-block waitlist stay plus the post-await segment; the comment cites a 1.10 node measurement without the waitlist term.
