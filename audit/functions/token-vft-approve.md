## `Vft::approve` in programs/demo-token/app/src/lib.rs (L188-L194)

**Purpose:** Exported VFT `Approve(spender, value) -> bool throws TokenError` (client IDL, programs/demo-token/client/demo_token_client.idl L19). The step a depositor performs so the vault's `transfer_from` can pull funds.

**Inputs & Assumptions:**
- `spender` (ActorId), `value` (U256): message payload. Trust: untrusted.
- Implicit: `owner = Syscall::message_source()` (L190) — the signer or calling program. `paused`.
- Preconditions: none.

**Outputs & Effects:**
- Success: allowance overwritten (L191 -> L108), `Approval { owner, spender, value }` emitted (L192), reply `true` (L193). Every success path returns `true`; `false` is never produced.
- Error: `Paused` or `ZeroAddress` from `TokenState::approve` (L106-L107). Because of `#[export(unwrap_result)]` (L188) the generated exposure wraps the call in `ok_or_throws!` (sails-macros-core-1.0.1/src/service/exposure.rs L97-L104), which encodes the error and calls `Syscall::panic` (sails-rs-1.0.1/src/gstd/macros.rs L307-L325, syscalls.rs L84-L86). The caller receives an error reply whose panic payload decodes back to `TokenError` (client/mod.rs L450-L464).

**Block-by-Block:**

```rust
// L190-L193
let owner = Syscall::message_source();
self.state.borrow_mut().approve(owner, spender, value)?;
self.emit_event(VftEvent::Approval { owner, spender, value }).expect("event");
Ok(true)
```
- **What:** identity, state transition, event, fixed reply.
- **Why here:** the `RefMut` is a temporary dropped at the end of L191, before the emit; the event is emitted only after the write succeeded.
- **Assumes:** the emit succeeds; otherwise `expect` panics and the handler aborts after the allowance write (see couplings).
- **Establishes:** `allowance(owner, spender) == value` and an `Approval` event with the same triple.
- **Depended on by:** `Vft::transfer_from` from `spender`.

**Cross-Function Dependencies:**
- Callee `TokenState::approve` (internal, L105-L110): validates and writes; errors before writing.
- Callee `emit_event` (external-source-available, events.rs L92-L101).
- Callers: any actor; in the vault flow, the depositor (vault lib.rs L736 comment "requires prior approval").
- Shared state: `allowances`.
- Invariant couplings: paused blocks approvals (L106). A panic after the write (emit failure) discards the write under Gear's atomic message execution — see open question in token-vft-transfer-from.md.

**Open Questions:**
- None beyond the shared atomicity question.
