## Withdrawal nullifier

Seed: 
```
"withdrawal_nullifier" + signer_address + H(s, step_count)
```

`H(s, step_count)` - is provided by the user as param.

Nullifier is used to prevent double spending, each pda address is the unique entry registering the withdrawal. If the same address would about to be used again the program would throw an error.
