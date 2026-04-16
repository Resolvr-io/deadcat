# Deterministic Reissuance Token Blinding for Simplicity Covenants

## Summary

Elements/Liquid requires reissuance token (RT) outputs to be blinded (confidential) for reissuance to function. The asset blinding factor (ABF) of the RT output serves as both a protocol signal (non-zero = reissuance, zero = new issuance) and an authorization mechanism (only someone who can reveal the ABF can reissue). This authorization model was designed for Bitcoin Script, where no other mechanism could restrict reissuance spending. With Simplicity covenants, the covenant itself can enforce spending conditions on RT UTXOs, making ABF secrecy redundant for authorization in covenants that fully restrict issuance. This document proposes a SimplicityHL convention for such covenants — using deterministic (publicly derivable) blinding factors for RT outputs, eliminating the need for out-of-band blinding data sharing — and an expansion of the `elements::PartiallySignedTransaction` API to support its easy and correct implementation.

## Background: Elements Reissuance Model

### Asset Issuance on Elements

Elements supports two kinds of asset issuance:

1. **New issuance**: Creates a new asset. The `assetBlindingNonce` field in the issuance input is set to all zeros. The asset ID is derived from the defining outpoint and contract hash.

2. **Reissuance**: Mints additional units of an existing asset. Requires spending a reissuance token UTXO. The `assetBlindingNonce` field must contain the asset blinding factor of the spent RT UTXO.

The protocol distinguishes these two cases by checking whether `assetBlindingNonce` is zero:

```
if assetBlindingNonce == 0x00...00:
    this is a new issuance — compute entropy from prevout + contract_hash
else:
    this is a reissuance — entropy is stored in asset_entropy field
```

### Why RT Outputs Must Be Blinded

During reissuance, consensus validates that the provided `assetBlindingNonce` matches the actual ABF of the spent RT UTXO:

1. Derive the expected blinded asset generator from `(reissuance_token_id, assetBlindingNonce)`
2. Compare against the spent UTXO's on-chain asset commitment
3. Reject if they don't match

If an RT output were explicit (unblinded), its ABF would be zero. Providing ABF=0 as the `assetBlindingNonce` would trigger the "new issuance" code path instead of reissuance — consensus rejection.

**Therefore, RT outputs must be blinded so their ABF is non-zero.**

### The ABF as Authorization (Pre-Simplicity)

In the Bitcoin Script model, the ABF serves a dual purpose:

1. **Protocol signal**: Non-zero ABF in `assetBlindingNonce` distinguishes reissuance from new issuance
2. **Authorization**: Only someone who can unblind the RT UTXO (i.e., knows the ABF) can construct a valid reissuance transaction. The ABF is effectively a bearer credential.

This coupling of type discrimination and authorization into a single field was a reasonable design pre-Simplicity, but it imposes constraints on covenant-based designs. Script could not express complex spending conditions like "only reissue if condition X is met" or "only reissue up to amount Y." The blinding factor's secrecy was the security model.

## The Problem: Simplicity Changes the Authorization Model

Simplicity covenants can enforce arbitrary spending conditions on RT UTXOs. A Simplicity program can restrict:

- Who can trigger reissuance (e.g., requiring a specific oracle signature)
- When reissuance can occur (e.g., time-locked, state-dependent)
- How much can be reissued (e.g., bounded by covenant parameters)
- What outputs the reissuance transaction must produce (e.g., enforcing that new tokens go to specific covenant addresses)

With these capabilities, the ABF is no longer needed for authorization — the covenant provides strictly stronger guarantees. However, the Elements protocol still requires a non-zero ABF for the reissuance mechanism to function. This creates a tension:

- The ABF must be non-zero (protocol requirement)
- The ABF need not be secret (covenant provides authorization)
- The standard tooling (`blind_last` / `blind_non_last`) generates random ABFs, assuming secrecy matters
- Random ABFs create an out-of-band secret sharing problem: anyone who wants to interact with the covenant-controlled asset must somehow obtain the blinding factors

### Concrete Impact

Consider a Simplicity covenant that manages reissuable assets — for example, a covenant that controls token issuance based on on-chain conditions. The covenant's RT UTXOs are locked by the Simplicity program; no one can spend them without satisfying the covenant.

With random blinding:
- The covenant creator generates random ABFs during the creation transaction
- These ABFs must be shared with every participant who needs to construct reissuance transactions through the covenant
- If the ABFs are lost, the reissuance tokens become unusable (even though they're visible on-chain and locked in a covenant)
- Discovery protocols must include ABF distribution alongside covenant parameters

With deterministic blinding:
- ABFs are derived from public on-chain data
- Anyone can compute them from the creation transaction
- No out-of-band secret sharing needed
- Discovery only requires covenant parameters — the ABFs are implicit

## Proposed Solution: Deterministic ABF Derivation

For Simplicity covenants that both define new assets and fully restrict their issuance, deterministic RT blinding should be the standard approach. The blinding factors serve no authorization purpose in these covenants — the Simplicity program is the sole gatekeeper — so they can be derived from public data without sacrificing security.

### Approach

Instead of random blinding factors, derive ABFs deterministically from public data using tagged hashes (following the BIP-340 convention):

```
RT_ABF = SHA256(SHA256(tag) || SHA256(tag) || defining_outpoint)
```

Where:
- `tag` is an application-specific string (e.g., `"myapp/rt_abf"`)
- `defining_outpoint` is the serialized outpoint of the issuance input (the UTXO whose spend defines the new asset)

The value blinding factor (VBF) can be derived similarly if the RT output's value is publicly known (typically 1 satoshi for RTs):

```
RT_VBF = SHA256(SHA256(vbf_tag) || SHA256(vbf_tag) || defining_outpoint)
```

### Properties

| Property | Satisfied? | Notes |
| -------- | ---------- | ----- |
| Non-zero ABF | Yes | SHA256 output is non-zero with probability 1 - 2^-256 |
| Unique per issuance | Yes | Defining outpoints are unique (spent UTXOs can't be reused) |
| Publicly derivable | Yes | Defining outpoints are visible in the creation transaction |
| Protocol compatible | Yes | Produces valid Pedersen commitments and range proofs |
| No out-of-band sharing | Yes | Anyone with the creation tx can compute the ABFs |

### Implementation

The `elements` crate's [`PartiallySignedTransaction`](https://docs.rs/elements/latest/elements/pset/struct.PartiallySignedTransaction.html) provides exactly two public blinding methods:

- **`blind_non_last`** — blinds outputs for non-final participants in a multi-party blinding protocol
- **`blind_last`** — blinds outputs for the final participant, balancing all blinding factors

Both methods always generate random `AssetBlindingFactor` values via `AssetBlindingFactor::new(rng)`. There is no parameter, code path, or configuration option to supply predetermined blinding factors. The only other public blinding-related method, `surjection_inputs`, is a read-only helper for constructing surjection proofs — it does not generate or accept blinding factors.

Because the existing API cannot produce deterministically-blinded outputs, the implementation requires bypassing it for RT outputs and manually constructing the blinded outputs at a lower level:

1. **Create RT outputs as unblinded placeholders** in the PSET (same as current practice)
2. **Do not mark RT outputs for automatic blinding** — leave `blinding_key` unset on these outputs
3. **Manually construct Pedersen commitments** for RT outputs using the deterministic ABFs and VBFs, via the `secp256k1-zkp` generator and commitment primitives
4. **Manually generate range proofs** for RT outputs using the `secp256k1-zkp` rangeproof API
5. **Manually generate surjection proofs** for RT outputs to prove the blinded asset is among the transaction's inputs
6. **Set the blinded fields** directly on the PSET output (`asset_comm`, `value_comm`, rangeproof, surjection proof)
7. **Mark remaining outputs** (wallet change, etc.) for `blind_last` as usual
8. **Call `blind_last`** for the remaining outputs — it handles VBF balancing for those outputs independently

The selective blinding infrastructure already exists — outputs without `blinding_key` set are untouched by `blind_last`. This is the standard pattern for fee outputs (always explicit) and other outputs that must remain unblinded. However, the manual construction of Pedersen commitments, range proofs, and surjection proofs is cryptographically sensitive code that operates directly on elliptic curve primitives. See [Recommendation for Elements / SimplicityHL Documentation](#recommendation-for-elements--simplicityhl-documentation) for why this should be addressed at the platform level.

### Reissuance Transaction Construction

When constructing a reissuance transaction that spends a deterministically-blinded RT:

1. Compute the ABF from the original defining outpoint using the same tagged hash
2. Set `assetBlindingNonce` to the computed ABF
3. Set `assetEntropy` to the stored entropy (same as with random blinding)
4. Consensus validates: the derived blinded generator from `(token_id, ABF)` matches the on-chain commitment

This is identical to the standard reissuance flow — the only difference is where the ABF comes from (deterministic derivation vs. stored secret).

## Security Considerations

### ABF Disclosure Does Not Weaken Covenant Security

The ABF for a deterministically-blinded RT is publicly computable. This means anyone can:
- Identify RT outputs on-chain (by verifying commitments against derived ABFs)
- Construct the `assetBlindingNonce` field for a reissuance input

However, constructing a valid reissuance input also requires **spending the RT UTXO**, which requires satisfying the Simplicity covenant. The ABF is necessary but not sufficient for reissuance — the covenant is the binding constraint.

This is analogous to how knowing a P2WSH script hash doesn't let you spend the output — you still need to satisfy the script. With Simplicity covenants, knowing the ABF doesn't let you reissue — you still need to satisfy the covenant program.

### When Deterministic Blinding Is NOT Appropriate

Deterministic blinding is appropriate when:
- RT UTXOs are locked by Simplicity covenants that enforce spending conditions
- The existence and nature of the RTs is public information (e.g., publicly announced protocols)

Deterministic blinding is NOT appropriate when:
- ABF secrecy is the primary authorization mechanism (no covenant, only Script)
- The existence of the RTs should be hidden from observers (privacy-sensitive issuances)
- The issuance is controlled by a multisig or other Script-based mechanism without Simplicity enforcement

### On-Chain Fingerprinting

Deterministically-blinded RT outputs are identifiable by anyone who knows the application's tag string and can observe the creation transaction's inputs. For publicly announced protocols, this is not a concern — the RTs are already public. For protocols where RT privacy matters, random blinding should be retained.

## Recommendation for Elements / SimplicityHL Documentation

The current Elements documentation and tooling assumes that RT blinding factors are secrets. This was correct in the Bitcoin Script era but is no longer universally true with Simplicity. We recommend:

1. **Document the deterministic blinding pattern** as a valid approach for Simplicity-controlled reissuable assets
2. **Clarify the dual role of the ABF** (protocol signal vs. authorization) and note that Simplicity covenants can replace the authorization role
3. **Add API support** in the Elements PSET blinding functions for predetermined blinding factors on specific outputs (see below)
4. **Provide examples** showing how Simplicity covenant authors can use deterministic blinding to simplify their protocols

### The Case for API Support

Any Simplicity covenant that manages reissuable assets — prediction markets, stablecoins, governance tokens, synthetic assets — faces the same choice: distribute random blinding factors out-of-band, or use deterministic blinding. Deterministic blinding is the natural fit for covenant-controlled issuance, where ABF secrecy provides no security benefit.

However, the current `elements` crate API makes deterministic blinding unnecessarily difficult. The only two public blinding methods (`blind_last` and `blind_non_last`) always generate random `AssetBlindingFactor` values internally. There is no parameter to supply predetermined factors. To use deterministic blinding, a covenant author must:

1. Construct Pedersen commitments manually using `secp256k1-zkp` generator and commitment primitives
2. Generate range proofs manually using the `secp256k1-zkp` rangeproof API
3. Generate surjection proofs manually to prove the blinded asset is among the transaction's inputs
4. Set the resulting cryptographic objects directly on PSET output fields

This is cryptographically sensitive code operating directly on elliptic curve primitives. Incorrect commitment construction, malformed range proofs, or invalid surjection proofs produce transactions that are silently rejected by consensus — failures that are difficult to diagnose. Every Simplicity covenant author who uses reissuable assets would need to independently implement and audit this same cryptographic plumbing.

The platform should provide this capability so that covenant authors don't have to. The pattern is expected to be common in the Simplicity ecosystem — any covenant managing reissuable assets benefits from deterministic blinding.

### Proposed API Addition

A method that accepts predetermined ABFs/VBFs for specific outputs, handling commitment construction, range proof generation, and surjection proof generation internally:

```rust
/// Blind a specific output with predetermined blinding factors.
///
/// This is useful for Simplicity covenants where the blinding factors
/// are deterministically derived from public data rather than random.
/// The output must have explicit asset and value set. After this call,
/// the output will have valid confidential commitments and proofs.
///
/// Call this BEFORE `blind_last` / `blind_non_last`. Outputs blinded
/// with this method should NOT have `blinding_key` set (they are
/// excluded from automatic blinding).
pub fn blind_output_with_factors<C: secp256k1_zkp::Signing>(
    &mut self,
    output_index: usize,
    asset_blinding_factor: AssetBlindingFactor,
    value_blinding_factor: ValueBlindingFactor,
    inp_txout_sec: &HashMap<usize, TxOutSecrets>,
    secp: &secp256k1_zkp::Secp256k1<C>,
) -> Result<(), PsetBlindError>;
```

This method would:
1. Construct the Pedersen commitment for the asset using the provided ABF
2. Construct the Pedersen commitment for the value using the provided VBF
3. Generate a valid range proof for the committed value
4. Generate a valid surjection proof from the transaction's inputs
5. Set all confidential fields on the PSET output
6. Account for the predetermined factors in the global scalar accumulator so that subsequent `blind_last` calls balance correctly

Covenant authors would then write:

```rust
// Deterministic blinding for covenant-controlled RT outputs
let abf = derive_abf_from_defining_outpoint(&outpoint);
let vbf = derive_vbf_from_defining_outpoint(&outpoint);
pset.blind_output_with_factors(rt_output_index, abf, vbf, &inp_secrets, &secp)?;

// Standard random blinding for wallet outputs (change, etc.)
pset.blind_last(&mut rng, &secp, &inp_secrets)?;
```

This keeps the cryptographic complexity inside the platform library where it belongs, while giving covenant authors the control they need.

## Example: Covenant-Controlled Token Issuance

Consider a Simplicity covenant that manages a reissuable asset — say, a prediction market that issues outcome tokens through a covenant-enforced issuance process.

**Without deterministic blinding:**
1. Market creator generates creation transaction with random ABFs for RT outputs
2. Creator must distribute ABFs to all participants (via a separate discovery channel)
3. Participants store ABFs locally to construct future reissuance transactions
4. If ABFs are lost from the discovery channel, new participants cannot interact with the market's issuance mechanism

**With deterministic blinding:**
1. Market creator generates creation transaction with deterministic ABFs (derived from defining outpoints)
2. Creator publishes only the covenant parameters — ABFs are implicit
3. Any participant computes ABFs on demand from the creation transaction (publicly visible on-chain)
4. No ABF distribution needed; no single point of failure for secret storage

The covenant's security guarantees are identical in both cases — the Simplicity program controls who can reissue, not the ABF. The difference is purely operational: deterministic blinding eliminates an unnecessary secret management burden.
