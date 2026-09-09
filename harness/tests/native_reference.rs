use audio_flow_harness::{
    native_reference,
    study::{self, Candidate, Mode},
};
#[test]
fn native_literal_gates_and_workload_evidence() {
    for candidate in [Candidate::FundspBlock, Candidate::Oddio] {
        for gate in native_reference::gates(candidate) {
            if candidate == Candidate::FundspBlock {
                assert!(gate.passed, "{gate:?}");
            } else if gate.id == "native-Once" && gate.partitions == vec![1; 8] {
                assert!(!gate.passed);
                assert_eq!(gate.first_mismatch, Some(7));
            } else {
                assert!(gate.passed, "{gate:?}");
            }
        }
        for w in study::workloads() {
            for block in [1, 7, 64, 257, 4096] {
                let admitted = study::check(&w, candidate, block);
                let c = if candidate == Candidate::Oddio && w.mode == Mode::Once {
                    assert_eq!(admitted.outcome, "capability_gap");
                    study::diagnose_oddio_once(&w, block)
                } else {
                    admitted
                };
                if w.mode == Mode::Bursts {
                    assert_eq!(c.outcome, "capability_gap");
                } else if candidate == Candidate::FundspBlock {
                    assert_eq!(c.outcome, "pass", "{c:?}");
                } else if w.mode == Mode::Once && block != 4096 {
                    assert_eq!(c.outcome, "fail");
                    assert!(c.first_mismatch.is_some());
                    assert!(c.failed_sample_bits.is_some());
                } else {
                    assert_eq!(c.outcome, "pass", "{c:?}");
                }
            }
        }
    }
}

#[test]
fn native_submission_rejections_and_zero_buffer_preserve_state() {
    use audio_flow_harness::{offline::*, study::Prepared};
    for candidate in [Candidate::FundspBlock, Candidate::Oddio] {
        let w = study::Workload::new("native-rejection", 2, 64, Mode::Loop, 128, 1);
        let mut bank = Prepared::new(&w, candidate).unwrap();
        let before = bank.observe();
        let bad = vec![Batch {
            frame: 3,
            revision: 0,
            commands: vec![Command::Start(0), Command::Start(2)],
        }];
        assert!(bank.enqueue(bad).is_err());
        assert_eq!(before, bank.observe());
        bank.enqueue(w.batches()).unwrap();
        bank.render_into(&mut [vec![]]).unwrap();
        assert_eq!(before, bank.observe());
        assert!(bank
            .enqueue(vec![Batch {
                frame: 5,
                revision: 0,
                commands: vec![Command::Start(0)]
            }])
            .is_err());
        let mut out = vec![vec![0.; 64]];
        bank.render_into(&mut out).unwrap();
        assert_eq!(out[0], w.expected());
        assert_eq!(bank.observe(), w.expected_state());
    }
}

#[test]
fn native_mix_preserves_nonassociative_declared_voice_order() {
    use audio_flow_harness::study::{Prepared, Workload};
    let mut w = Workload::new("native-order", 3, 18, Mode::Loop, 128, 1);
    w.samples.fill(0.);
    w.samples[1] = 26.;
    w.samples[2] = -4e20;
    w.samples[3] = 2e20;
    assert_eq!(w.expected()[5].to_bits(), 3.25_f32.to_bits());
    for candidate in [Candidate::FundspBlock, Candidate::Oddio] {
        for block in [1, 6] {
            let mut bank = Prepared::new(&w, candidate).unwrap();
            bank.enqueue(w.batches()).unwrap();
            let mut actual = Vec::new();
            for _ in 0..18 / block {
                let mut out = vec![vec![0.; block]];
                bank.render_into(&mut out).unwrap();
                actual.extend_from_slice(&out[0]);
            }
            assert_eq!(
                study::first_mismatch(&w.expected(), &actual),
                None,
                "{candidate:?} {actual:?}"
            );
        }
    }
}
