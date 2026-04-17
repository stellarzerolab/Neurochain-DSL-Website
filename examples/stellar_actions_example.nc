# Manual action-plan example for the Soroban adapter
# These lines are parsed by parse_action_plan_from_nc() and printed as JSON by neurochain-stellar.

stellar.account.balance account="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" asset="XLM"

# stellar.account.create destination="GCU2P57LBRHW2ZHWDRAB7MXOQRTB2B5XBOR3YNZX6LWDWWT2GJ2UE3QS" starting_balance="2"
# stellar.account.fund_testnet account="GCU2P57LBRHW2ZHWDRAB7MXOQRTB2B5XBOR3YNZX6LWDWWT2GJ2UE3QS"

#stellar.change_trust asset_code="USDC" asset_issuer="GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5" limit="1000"

stellar.payment to="GCU2P57LBRHW2ZHWDRAB7MXOQRTB2B5XBOR3YNZX6LWDWWT2GJ2UE3QS" amount="5" asset_code="XLM"
# (enable after you actually hold USDC)
# stellar.payment to="GCU2P57LBRHW2ZHWDRAB7MXOQRTB2B5XBOR3YNZX6LWDWWT2GJ2UE3QS" amount="12.5" asset_code="USDC" asset_issuer="GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"

# stellar.tx.status hash="ABC123"

# soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P","amount":100}
