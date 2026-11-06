// Initialize the pool once per deployment: sends `initialize`, which creates program_config,
// the vault and the root registry (empty depth-20 tree). The signer becomes the pool owner,
// the only key that can pause and unpause it.
//
//   yarn install
//   yarn initialize-pool
//
// Env:
//   RPC_URL        default http://127.0.0.1:8899 (a local surfnet / validator)
//   OWNER_KEYPAIR  Solana CLI JSON keypair file, default ~/.config/solana/id.json
//
// Safe to run again: if program_config already exists, it only prints the current state.
// Reads the IDL and TS types from target/, so run `anchor2 build` first.

import * as anchor from "@coral-xyz/anchor";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import idl from "../target/idl/zk_shielded_pool_solana.json";
import type { ZkShieldedPoolSolana } from "../target/types/zk_shielded_pool_solana";

const { Connection, Keypair, PublicKey } = anchor.web3;

const rpcUrl = process.env.RPC_URL ?? "http://127.0.0.1:8899";
const ownerPath = process.env.OWNER_KEYPAIR ?? join(homedir(), ".config/solana/id.json");

function loadKeypair(path: string): anchor.web3.Keypair {
  const secret: number[] = JSON.parse(readFileSync(path, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

async function main() {
  const owner = loadKeypair(ownerPath);
  const connection = new Connection(rpcUrl, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(owner), {
    commitment: "confirmed",
  });
  const program = new anchor.Program<ZkShieldedPoolSolana>(idl as ZkShieldedPoolSolana, provider);

  const [programConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("program_config")],
    program.programId,
  );

  if (await connection.getAccountInfo(programConfig)) {
    console.log(`pool already initialized on ${rpcUrl}`);
  } else {
    console.log(`initializing pool on ${rpcUrl} with owner ${owner.publicKey.toBase58()}`);
    // program_config, vault and root_registry are PDAs from the IDL seeds: Anchor derives them.
    const signature = await program.methods
      .initialize()
      .accountsPartial({ signer: owner.publicKey })
      .rpc();
    console.log(`initialize tx: ${signature}`);
  }

  const config = await program.account.programConfig.fetch(programConfig);
  console.log(`owner: ${config.owner.toBase58()}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
