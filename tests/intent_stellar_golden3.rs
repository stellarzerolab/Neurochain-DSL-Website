use neurochain::ai::model::AIModel;
use neurochain::intent_stellar::{build_action_plan, IntentDecision, IntentStellarLabel};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Case {
    text: &'static str,
    primary_label: &'static str,
    accepted_labels: &'static [&'static str],
    accepted_action_kinds: &'static [&'static str],
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
fn intent_stellar_golden3_round3() {
    let model_path = stellar_model_path();
    if !model_path.exists() {
        eprintln!(
            "intent_stellar_golden3_round3 skipped: model not found at {}",
            model_path.display()
        );
        return;
    }

    let model = AIModel::new(model_path.to_string_lossy().as_ref()).expect("intent_stellar loads");

    let cases: &[Case] = &[
        Case {
            text: "check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM",
            primary_label: "BalanceQuery",
            accepted_labels: &["BalanceQuery"],
            accepted_action_kinds: &["stellar.account.balance"],
            min_score: 0.30,
        },
        Case {
            text: "check balnce for GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ in xIm pls !!!",
            primary_label: "BalanceQuery",
            accepted_labels: &["BalanceQuery"],
            accepted_action_kinds: &["stellar.account.balance"],
            min_score: 0.18,
        },
        Case {
            text: "create acccount GC6FLLQELHZ3GXDXFIIMJ477E2FVEI2PFSM4AD4IXELERA4ZUUPVLBLQ with startng balance 2.5 XLM",
            primary_label: "CreateAccount",
            accepted_labels: &["CreateAccount", "TransferXLM"],
            accepted_action_kinds: &["stellar.account.create", "stellar.payment", "unknown"],
            min_score: 0.18,
        },
        Case {
            text: "add trstline USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 limt 700",
            primary_label: "ChangeTrust",
            accepted_labels: &["ChangeTrust", "TransferAsset"],
            accepted_action_kinds: &["stellar.change_trust", "stellar.payment", "unknown"],
            min_score: 0.18,
        },
        Case {
            text: "sendd 5 xIm to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P !!!",
            primary_label: "TransferXLM",
            accepted_labels: &["TransferXLM", "TransferAsset"],
            accepted_action_kinds: &["stellar.payment", "unknown"],
            min_score: 0.15,
        },
        Case {
            text: "send 12.5 TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GC5IPWDT6CXXHPQVOETX2AHKABSSSYOSZFWGOLCRJM56AFMDSTMLV3KL",
            primary_label: "TransferAsset",
            accepted_labels: &["TransferAsset"],
            accepted_action_kinds: &["stellar.payment"],
            min_score: 0.30,
        },
        Case {
            text: "fund testnet account GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX",
            primary_label: "FundTestnet",
            accepted_labels: &["FundTestnet"],
            accepted_action_kinds: &["stellar.account.fund_testnet"],
            min_score: 0.28,
        },
        Case {
            text: "check tx sttus f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15",
            primary_label: "TxStatus",
            accepted_labels: &["TxStatus"],
            accepted_action_kinds: &["stellar.tx.status"],
            min_score: 0.28,
        },
        Case {
            text: "invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={\"to\":\"GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ\",\"amount\":100} arg_types={\"to\":\"address\",\"amount\":\"u64\"}",
            primary_label: "ContractInvoke",
            accepted_labels: &["ContractInvoke"],
            accepted_action_kinds: &["soroban.contract.invoke"],
            min_score: 0.25,
        },
        Case {
            text: "execute transfer on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ args={\"to\":\"World\",\"amount\":10} arg_types={\"to\":\"address\",\"amount\":\"u64\"}",
            primary_label: "ContractInvoke",
            accepted_labels: &["ContractInvoke"],
            accepted_action_kinds: &["soroban.contract.invoke", "unknown"],
            min_score: 0.18,
        },
        Case {
            text: "call set_blob on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ args={\"blob\":\"0xZZ11\"} arg_types={\"blob\":\"bytes\"}",
            primary_label: "ContractInvoke",
            accepted_labels: &["ContractInvoke"],
            accepted_action_kinds: &["soroban.contract.invoke", "unknown"],
            min_score: 0.18,
        },
        Case {
            text: "run weather forecast for helsinki",
            primary_label: "Unknown",
            accepted_labels: &["Unknown"],
            accepted_action_kinds: &["unknown"],
            min_score: 0.15,
        },
        Case {
            text: "execute local backup process",
            primary_label: "Unknown",
            accepted_labels: &["Unknown"],
            accepted_action_kinds: &["unknown"],
            min_score: 0.15,
        },
        // Robust punctuation / unicode / long prompts
        Case {
            text: "PLEASE!!! sendd 3.0 xIm to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ ... now???",
            primary_label: "TransferXLM",
            accepted_labels: &["TransferXLM", "TransferAsset"],
            accepted_action_kinds: &["stellar.payment", "unknown"],
            min_score: 0.15,
        },
        Case {
            text: "could you maybe maybe check tx status deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef 🙏",
            primary_label: "TxStatus",
            accepted_labels: &["TxStatus", "Unknown"],
            accepted_action_kinds: &["stellar.tx.status", "unknown"],
            min_score: 0.15,
        },
        Case {
            text: "please, without any extra words, create acccount GC6FLLQELHZ3GXDXFIIMJ477E2FVEI2PFSM4AD4IXELERA4ZUUPVLBLQ with startng balance 1.25 xlm and keep output concise",
            primary_label: "CreateAccount",
            accepted_labels: &["CreateAccount", "TransferXLM"],
            accepted_action_kinds: &["stellar.account.create", "stellar.payment", "unknown"],
            min_score: 0.15,
        },
        // Multi-intent ambiguities
        Case {
            text: "check balance for GC5IPWDT6CXXHPQVOETX2AHKABSSSYOSZFWGOLCRJM56AFMDSTMLV3KL and then send 1 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P",
            primary_label: "BalanceQuery",
            accepted_labels: &["BalanceQuery", "TransferXLM", "TransferAsset"],
            accepted_action_kinds: &["stellar.account.balance", "stellar.payment", "unknown"],
            min_score: 0.15,
        },
        Case {
            text: "run tx lookup deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef and then friendbot GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ",
            primary_label: "TxStatus",
            accepted_labels: &["TxStatus", "FundTestnet"],
            accepted_action_kinds: &["stellar.tx.status", "stellar.account.fund_testnet", "unknown"],
            min_score: 0.15,
        },
        Case {
            text: "run hello on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ and then send 1 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P",
            primary_label: "ContractInvoke",
            accepted_labels: &["ContractInvoke", "TransferXLM", "TransferAsset"],
            accepted_action_kinds: &["soroban.contract.invoke", "stellar.payment", "unknown"],
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
                "run {pass} case {i}: label={label} score={score:.3} latency_ms={ms:.2} primary={} accepted={:?} min={} | {:?}",
                c.primary_label, c.accepted_labels, c.min_score, c.text
            );

            assert!(
                c.accepted_labels.contains(&label.as_str()),
                "pass {pass} case {i} label mismatch for input: {:?} (got={label}, primary={}, accepted={:?}, score={score:.3})",
                c.text,
                c.primary_label,
                c.accepted_labels
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
            let kind = plan.actions[0].kind();
            assert!(
                c.accepted_action_kinds.contains(&kind),
                "pass {pass} case {i} action kind mismatch: got={kind}, accepted={:?}",
                c.accepted_action_kinds
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
