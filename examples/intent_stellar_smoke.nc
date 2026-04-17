# IntentStellar smoke test
# Labels:
# BalanceQuery, CreateAccount, ChangeTrust, TransferXLM, TransferAsset,
# FundTestnet, TxStatus, ContractInvoke, Unknown

AI: "models/intent_stellar/model.onnx"
neuro "Starting IntentStellar smoke test"

set q1 from AI: "Check balance of XLM for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
neuro q1

set q2 from AI: "Create account GDYP6UAEMRNW6UQFHHTUNX7QAKDWUTXQCYEHRPQNSW624A522MMEGYOH with 2 XLM"
neuro q2

set q3 from AI: "Add trustline TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX limit 1000"
neuro q3

set q4 from AI: "Send 5 XLM to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ"
neuro q4

set q5 from AI: "Send 12.5 TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ"
neuro q5

set q6 from AI: "Fund testnet account GC5IPWDT6CXXHPQVOETX2AHKABSSSYOSZFWGOLCRJM56AFMDSTMLV3KL"
neuro q6

set q7 from AI: "Check status of tx f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15"
neuro q7

set q8 from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello"
neuro q8

set q9 from AI: "Tell me a joke about rockets"
neuro q9

neuro "IntentStellar smoke test completed"
