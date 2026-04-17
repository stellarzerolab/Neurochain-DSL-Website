use neurochain::actions::Action;
use neurochain::intent_stellar::{
    build_action_plan, has_intent_blocking_issue, IntentDecision, IntentStellarLabel,
};

fn decision(label: IntentStellarLabel) -> IntentDecision {
    IntentDecision {
        label,
        score: 0.95,
        threshold: 0.55,
        downgraded_to_unknown: false,
    }
}

fn assert_no_intent_error(plan: &neurochain::actions::ActionPlan) {
    assert!(
        !plan.warnings.iter().any(|w| w.starts_with("intent_error:")),
        "unexpected intent_error warnings: {:?}",
        plan.warnings
    );
}

#[test]
fn intent_stellar_template_mapping_happy_paths() {
    let g1 = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let g2 = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
    let c1 = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let hash = "f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15";

    let plan = build_action_plan(
        &format!("Check balance for {g1} asset XLM"),
        &decision(IntentStellarLabel::BalanceQuery),
    );
    assert_eq!(plan.actions.len(), 1);
    match &plan.actions[0] {
        Action::StellarAccountBalance { account, asset } => {
            assert_eq!(account, g1);
            assert_eq!(asset.as_deref(), Some("XLM"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Create account {g2} with starting balance 2"),
        &decision(IntentStellarLabel::CreateAccount),
    );
    match &plan.actions[0] {
        Action::StellarAccountCreate {
            destination,
            starting_balance,
        } => {
            assert_eq!(destination, g2);
            assert_eq!(starting_balance, "2");
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Add trustline TESTUSD:{g1} limit 1000"),
        &decision(IntentStellarLabel::ChangeTrust),
    );
    match &plan.actions[0] {
        Action::StellarChangeTrust {
            asset_code,
            asset_issuer,
            limit,
        } => {
            assert_eq!(asset_code, "TESTUSD");
            assert_eq!(asset_issuer, g1);
            assert_eq!(limit.as_deref(), Some("1000"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Send 5 XLM to {g2}"),
        &decision(IntentStellarLabel::TransferXLM),
    );
    match &plan.actions[0] {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            assert_eq!(to, g2);
            assert_eq!(amount, "5");
            assert_eq!(asset_code, "XLM");
            assert!(asset_issuer.is_none());
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Send 12.5 TESTUSD:{g1} to {g2}"),
        &decision(IntentStellarLabel::TransferAsset),
    );
    match &plan.actions[0] {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            assert_eq!(to, g2);
            assert_eq!(amount, "12.5");
            assert_eq!(asset_code, "TESTUSD");
            assert_eq!(asset_issuer.as_deref(), Some(g1));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Fund testnet account {g1}"),
        &decision(IntentStellarLabel::FundTestnet),
    );
    match &plan.actions[0] {
        Action::StellarAccountFundTestnet { account } => assert_eq!(account, g1),
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Check tx status {hash}"),
        &decision(IntentStellarLabel::TxStatus),
    );
    match &plan.actions[0] {
        Action::StellarTxStatus { hash: got_hash } => assert_eq!(got_hash, hash),
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Invoke contract {c1} function transfer args={{\"to\":\"{g2}\",\"amount\":5}}"),
        &decision(IntentStellarLabel::ContractInvoke),
    );
    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, c1);
            assert_eq!(function, "transfer");
            assert_eq!(args["to"].as_str(), Some(g2));
            assert_eq!(args["amount"].as_i64(), Some(5));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);
}

#[test]
fn intent_stellar_slot_missing_is_blocking_unknown() {
    let g2 = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
    let c1 = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";

    let cases = [
        (IntentStellarLabel::TransferXLM, format!("Send XLM to {g2}")),
        (
            IntentStellarLabel::ChangeTrust,
            "Add trustline USDC limit 100".to_string(),
        ),
        (
            IntentStellarLabel::CreateAccount,
            "Create account with starting balance 2".to_string(),
        ),
        (
            IntentStellarLabel::TxStatus,
            "Show latest tx status".to_string(),
        ),
        (
            IntentStellarLabel::ContractInvoke,
            format!("Invoke contract {c1} args={{\"to\":\"World\"}}"),
        ),
    ];

    for (label, prompt) in cases {
        let plan = build_action_plan(&prompt, &decision(label));
        assert_eq!(plan.actions.len(), 1);
        assert!(has_intent_blocking_issue(&plan));
        match &plan.actions[0] {
            Action::Unknown { reason } => {
                assert!(
                    reason.starts_with("slot_missing:"),
                    "expected slot_missing reason, got: {reason}"
                );
            }
            other => panic!("expected Unknown action, got: {other:?}"),
        }
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.starts_with("intent_error: slot_missing:")),
            "missing slot_missing warning: {:?}",
            plan.warnings
        );
    }
}

#[test]
fn intent_stellar_contract_invoke_typed_validation_blocks_type_errors() {
    let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let g1 = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";

    let ok_prompt = format!(
        "Invoke contract {contract} function transfer args={{\"to\":\"{g1}\",\"blob\":\"0x0A0B\",\"ticker\":\"USDC\",\"amount\":100}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}"
    );
    let ok_plan = build_action_plan(&ok_prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert_eq!(ok_plan.actions.len(), 1);
    assert_eq!(ok_plan.actions[0].kind(), "soroban.contract.invoke");
    assert!(!has_intent_blocking_issue(&ok_plan));
    assert!(ok_plan
        .warnings
        .iter()
        .all(|w| !w.starts_with("intent_error: slot_type_error")));

    let bad_prompt = format!(
        "Invoke contract {contract} function transfer args={{\"to\":\"World\",\"amount\":-1}} arg_types={{\"to\":\"address\",\"amount\":\"u64\"}}"
    );
    let bad_plan = build_action_plan(&bad_prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&bad_plan));
    match &bad_plan.actions[0] {
        Action::Unknown { reason } => {
            assert!(
                reason.starts_with("slot_type_error:"),
                "expected slot_type_error reason, got: {reason}"
            );
        }
        other => panic!("expected Unknown action, got: {other:?}"),
    }
    assert!(bad_plan
        .warnings
        .iter()
        .any(|w| w.starts_with("intent_error: slot_type_error:")));
}

#[test]
fn intent_stellar_low_confidence_downgrade_is_blocking_unknown() {
    let decision = IntentDecision {
        label: IntentStellarLabel::Unknown,
        score: 0.20,
        threshold: 0.55,
        downgraded_to_unknown: true,
    };
    let plan = build_action_plan("Send 5 XLM to G...", &decision);
    assert!(has_intent_blocking_issue(&plan));
    match &plan.actions[0] {
        Action::Unknown { reason } => {
            assert!(reason.starts_with("intent_low_confidence:"));
        }
        other => panic!("expected Unknown action, got: {other:?}"),
    }
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.starts_with("intent_warning: low_confidence")),
        "missing low confidence warning: {:?}",
        plan.warnings
    );
}
