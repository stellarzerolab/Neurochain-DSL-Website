use neurochain::ai::model::AIModel;
use neurochain::intent_stellar::{build_action_plan, IntentDecision, IntentStellarLabel};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Case {
    text: &'static str,
    expected_label: &'static str,
    expected_action_kind: &'static str,
    min_score: f32,
}

fn stellar_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("models");
            path
        });

    base.join("intent_stellar").join("model.onnx")
}

fn decision_from_prediction(label: &str, score: f32) -> IntentDecision {
    IntentDecision {
        label: IntentStellarLabel::from_label(label),
        score,
        threshold: 0.0,
        downgraded_to_unknown: false,
    }
}

#[test]
fn intent_stellar_golden() {
    let model_path = stellar_model_path();
    if !model_path.exists() {
        eprintln!(
            "intent_stellar_golden skipped: model not found at {}",
            model_path.display()
        );
        return;
    }

    let model = AIModel::new(model_path.to_string_lossy().as_ref()).expect("intent_stellar loads");

    let cases: &[Case] = &[
        // BalanceQuery
        Case {
            text: "Check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM",
            expected_label: "BalanceQuery",
            expected_action_kind: "stellar.account.balance",
            min_score: 0.30,
        },
        Case {
            text: "Show XLM balance for GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "BalanceQuery",
            expected_action_kind: "stellar.account.balance",
            min_score: 0.30,
        },
        // CreateAccount
        Case {
            text: "Create account GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ with 2 XLM",
            expected_label: "CreateAccount",
            expected_action_kind: "stellar.account.create",
            min_score: 0.30,
        },
        Case {
            text: "Create account GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ with starting balance 5",
            expected_label: "CreateAccount",
            expected_action_kind: "stellar.account.create",
            min_score: 0.30,
        },
        // ChangeTrust
        Case {
            text: "Add trustline TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX limit 1000",
            expected_label: "ChangeTrust",
            expected_action_kind: "stellar.change_trust",
            min_score: 0.30,
        },
        Case {
            text: "Change trust USDC:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX limit 500",
            expected_label: "ChangeTrust",
            expected_action_kind: "stellar.change_trust",
            min_score: 0.30,
        },
        // TransferXLM
        Case {
            text: "Send 5 XLM to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "TransferXLM",
            expected_action_kind: "stellar.payment",
            min_score: 0.30,
        },
        Case {
            text: "Transfer 12 XLM to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "TransferXLM",
            expected_action_kind: "stellar.payment",
            min_score: 0.30,
        },
        // TransferAsset
        Case {
            text: "Send 15 TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "TransferAsset",
            expected_action_kind: "stellar.payment",
            min_score: 0.30,
        },
        Case {
            text: "Transfer 1.5 USDC:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "TransferAsset",
            expected_action_kind: "stellar.payment",
            min_score: 0.30,
        },
        // FundTestnet
        Case {
            text: "Fund testnet account GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX",
            expected_label: "FundTestnet",
            expected_action_kind: "stellar.account.fund_testnet",
            min_score: 0.30,
        },
        Case {
            text: "Fund testnet account GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            expected_label: "FundTestnet",
            expected_action_kind: "stellar.account.fund_testnet",
            min_score: 0.30,
        },
        // TxStatus
        Case {
            text: "Check tx status f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15",
            expected_label: "TxStatus",
            expected_action_kind: "stellar.tx.status",
            min_score: 0.30,
        },
        Case {
            text: "Transaction status for hash f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15",
            expected_label: "TxStatus",
            expected_action_kind: "stellar.tx.status",
            min_score: 0.30,
        },
        // ContractInvoke
        Case {
            text: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"World\"}",
            expected_label: "ContractInvoke",
            expected_action_kind: "soroban.contract.invoke",
            min_score: 0.30,
        },
        Case {
            text: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={\"to\":\"GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ\",\"amount\":100} arg_types={\"to\":\"address\",\"amount\":\"u64\"}",
            expected_label: "ContractInvoke",
            expected_action_kind: "soroban.contract.invoke",
            min_score: 0.30,
        },
        // Unknown
        Case {
            text: "Tell me a joke about stars",
            expected_label: "Unknown",
            expected_action_kind: "unknown",
            min_score: 0.15,
        },
        Case {
            text: "How is the weather in Helsinki today?",
            expected_label: "Unknown",
            expected_action_kind: "unknown",
            min_score: 0.15,
        },
    ];

    fn run_pass(model: &AIModel, cases: &[Case], pass: usize) -> (Vec<f64>, Duration) {
        let mut total = Duration::from_secs(0);
        let mut per_case_ms: Vec<f64> = Vec::with_capacity(cases.len());

        for (i, c) in cases.iter().enumerate() {
            let started = Instant::now();
            let (label, score) = model
                .predict_with_score(c.text)
                .unwrap_or_else(|e| panic!("pass {pass} case {i} predict failed: {e}"));
            let elapsed = started.elapsed();
            total += elapsed;

            let ms = elapsed.as_secs_f64() * 1000.0;
            per_case_ms.push(ms);
            println!(
                "run {pass} case {i}: label={label} score={score:.3} latency_ms={ms:.2} expected={} min={} | {:?}",
                c.expected_label, c.min_score, c.text
            );

            assert_eq!(
                label, c.expected_label,
                "pass {pass} case {i} label mismatch for input: {:?} (score={score:.3})",
                c.text
            );
            assert!(
                score >= c.min_score,
                "pass {pass} case {i} score too low for input: {:?} (label={label}, score={score:.3}, min={})",
                c.text,
                c.min_score
            );

            let decision = decision_from_prediction(&label, score);
            let plan = build_action_plan(c.text, &decision);
            assert_eq!(
                plan.actions.len(),
                1,
                "pass {pass} case {i} expected one action"
            );
            assert_eq!(
                plan.actions[0].kind(),
                c.expected_action_kind,
                "pass {pass} case {i} unexpected action kind"
            );
        }

        let avg_ms = (total.as_secs_f64() * 1000.0) / (cases.len().max(1) as f64);
        println!(
            "run {pass} summary: cases={} total_ms={:.2} avg_ms={:.2}",
            cases.len(),
            total.as_secs_f64() * 1000.0,
            avg_ms
        );

        (per_case_ms, total)
    }

    let (ms1, total1) = run_pass(&model, cases, 1);
    let (ms2, total2) = run_pass(&model, cases, 2);

    let t1 = total1.as_secs_f64() * 1000.0;
    let t2 = total2.as_secs_f64() * 1000.0;
    let avg1 = t1 / (cases.len().max(1) as f64);
    let avg2 = t2 / (cases.len().max(1) as f64);
    println!(
        "warmup delta: total_ms {:.2} -> {:.2} (diff {:+.2}), avg_ms {:.2} -> {:.2} (diff {:+.2})",
        t1,
        t2,
        t2 - t1,
        avg1,
        avg2,
        avg2 - avg1
    );

    for i in 0..cases.len().min(ms1.len()).min(ms2.len()) {
        println!(
            "case {i} delta_ms: run1={:.2} run2={:.2} diff={:+.2}",
            ms1[i],
            ms2[i],
            ms2[i] - ms1[i]
        );
    }
}
