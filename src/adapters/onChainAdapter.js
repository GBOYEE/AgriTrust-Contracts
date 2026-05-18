"use strict";

const { SorobanRpc, Contract, xdr } = require("@stellar/stellar-sdk");

const RPC_URL = process.env.STELLAR_RPC_URL || "https://rpc.mainnet.stellar.org";
const CONTRACT_ID = process.env.GRANT_STREAM_CONTRACT_ID;

let server;
let contract;

function getServer() {
  if (!server) server = new SorobanRpc.Server(RPC_URL);
  return server;
}

function getContract() {
  if (!contract && CONTRACT_ID) contract = new Contract(CONTRACT_ID);
  return contract;
}

const onChainAdapter = {
  async getEscrow(escrowId) {
    if (!CONTRACT_ID) throw new Error("GRANT_STREAM_CONTRACT_ID not set");
    const result = await getServer().simulateInvoke({
      contract: getContract().address(),
      method: "read_escrow",
      args: [xdr.ScVal.scvSymbol(escrowId)],
    });
    return result;
  },

  async getLegalHold(escrowId) {
    if (!CONTRACT_ID) throw new Error("GRANT_STREAM_CONTRACT_ID not set");
    const result = await getServer().simulateInvoke({
      contract: getContract().address(),
      method: "get_legal_hold",
      args: [xdr.ScVal.scvSymbol(escrowId)],
    });
    return result;
  },
};

module.exports = { onChainAdapter };
