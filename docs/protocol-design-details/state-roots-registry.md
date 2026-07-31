## Roots Registry

PDA that stores a ring buffer of archived Merkle roots for the shielded pool.

On each deposit made event with deposit commitment is emitted
```
Deposit commitment: H_final(H_user, total_amount)
```
