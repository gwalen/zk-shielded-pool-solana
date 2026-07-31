## Deposit

This function is called when a user deposits funds into the pool and creates a commitment hash.

### Will take the following parameters:
- H_user: user commitment hash
- total_amount: total amount of the deposit to by put in the vault


### Effect of this instruction:
- calculate final deposit commitment hash : H_final(H_user, total_amount)
- on-chain will add this node to IMT tree and modify root registry calculating new root hash
  - user may also submit it root (for comparison) or deposit will return or log new root hash
  - user needs save this root for later merkle proof generation submitted when withdraw (merkle proof is validated in the verifier)
- add new root (current root) to ring buffer of roots registry
