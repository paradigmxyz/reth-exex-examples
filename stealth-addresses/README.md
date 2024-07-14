# Stealth addresses

Scans committed blocks for stealth addresses according to [ERC5564](https://eips.ethereum.org/EIPS/eip-5564).

## Node

```bash
export VIEW_KEY=0x...
stealthy node
```

## Generation

```bash
Usage: stealthy gen [OPTIONS] <COMMAND>

Commands:
  addr  Generate a stealth address from a stealth meta address alongside an optional encrypted note
  meta  Generate a stealth meta address from view and spend private keys
  key   Generate the stealth address private key from the ephemeral public key and view & spend private keys
```
