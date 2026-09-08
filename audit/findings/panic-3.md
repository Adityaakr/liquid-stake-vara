# panic-3: Compensation path discards the `Result` of `mint_shares` (`let _ =`)

- **Location**
  - `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:414` (`revert_redeem`, called from `Vault::redeem` after a failed `push_underlying`)
  - `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:387` (`revert_unstake`, called from `Pool::unstake` after a failed `pay_out`)
- **Bug class**: RESDISC (silently discarded `Result`)
- **Severity**: Informational
- **Confidence**: High that the discard exists; the failing branch is practically unreachable

## Description

```rust
pub fn revert_redeem(&mut self, owner: ActorId, shares: U256, p: &RedeemPreview) {
    let _ = self.mint_shares(owner, shares);
    ...
}
```

`mint_shares` can fail with `Overflow` from `total_shares.checked_add` or `credit`'s
`checked_add`. If it did, the compensation would restore `fees_accrued` and `holdings`
but *not* the owner's shares, so the redeemer would lose their position while the assets
stay in the vault (socialised to the remaining holders). The failure needs
`total_shares + shares` or `balance + shares` to exceed `U256::MAX`; since `shares` was just
burned from those same totals, that requires ~2^256 shares to have been minted by
interleaving deposits in the await window (vault) or is impossible in the same execution
(pool). Filed per the result-discarded rule (every confirmed silent discard is recorded).

## Concrete trigger

Not practically reachable. Needs `U256` share supply near `U256::MAX`.

## Impact

None in practice; if it fired, the user's shares would be lost without an error.

## Suggested fix

Make the compensation infallible by construction: use a dedicated `restore_shares` that
writes back the pre-burn balance and supply captured in `begin_redeem` (no arithmetic),
or propagate the error and surface it in the reply (`Err(VaultError::Overflow)`) so the
condition is at least observable. Same for `revert_unstake` in the pool.
