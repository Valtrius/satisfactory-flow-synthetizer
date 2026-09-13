//! Exact Boolean counts with bounded prefix thresholds.
use std::fmt::Write;

/// Each threshold q[i,j] is equivalent to at least j true literals in the
/// first i positions. Both directions of the recurrence are asserted, so
/// auxiliary assignments neither add nor remove edge assignments. The last
/// prefix has threshold k true and k+1 false. Counting the complement keeps
/// the threshold width bounded by min(k, n-k)+1.
pub(crate) fn exactly(script: &mut String, prefix: &str, literals: &[String], count: usize) {
    if count > literals.len() {
        writeln!(script, "(assert false)").unwrap();
        return;
    }
    let complement = count > literals.len() / 2;
    let count = if complement {
        literals.len() - count
    } else {
        count
    };
    let mut previous = vec!["true".to_owned()];
    for (i, literal) in literals.iter().enumerate() {
        let literal = if complement {
            format!("(not {literal})")
        } else {
            literal.clone()
        };
        if count == 0 {
            writeln!(script, "(assert (not {literal}))").unwrap();
            continue;
        }
        let mut current = vec!["true".to_owned()];
        for j in 1..=(i + 1).min(count + 1) {
            let name = format!("q{prefix}_{i}_{j}");
            let stay = previous.get(j).map_or("false", String::as_str);
            let advance = &previous[j - 1];
            writeln!(
                script,
                "(declare-fun {name} () Bool)\n(assert (= {name} (or {stay} (and {advance} {literal}))))"
            )
            .unwrap();
            current.push(name);
        }
        previous = current;
    }
    if count > 0 {
        writeln!(
            script,
            "(assert {})\n(assert (not {}))",
            previous[count],
            previous[count + 1]
        )
        .unwrap();
    }
}

#[cfg(all(test, feature = "native-runtime"))]
mod tests {
    use super::*;
    use crate::process::Session;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn projected_models_match_every_small_truth_assignment() {
        let cancel = AtomicBool::new(false);
        for width in 0..=6 {
            let literals = (0..width).map(|i| format!("x{i}")).collect::<Vec<_>>();
            for count in 0..=width + 1 {
                let mut script = "(set-logic QF_LRA)\n".to_owned();
                for literal in &literals {
                    writeln!(script, "(declare-fun {literal} () Bool)").unwrap();
                }
                exactly(&mut script, "test", &literals, count);
                let mut session = Session::new().unwrap();
                session.write(&script).unwrap();
                for bits in 0u32..1 << width {
                    let mut assignment = "(push 1)\n".to_owned();
                    for (i, literal) in literals.iter().enumerate() {
                        writeln!(
                            assignment,
                            "(assert (= {literal} {}))",
                            bits & (1 << i) != 0
                        )
                        .unwrap();
                    }
                    assignment.push_str("(check-sat)\n");
                    session.write(&assignment).unwrap();
                    assert_eq!(
                        session.response(&cancel, &cancel).unwrap(),
                        if bits.count_ones() as usize == count {
                            "sat"
                        } else {
                            "unsat"
                        },
                        "width={width}, count={count}, bits={bits}"
                    );
                    session.write("(pop 1)\n").unwrap();
                }
            }
        }
    }
}
