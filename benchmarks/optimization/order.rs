/// Reorder first-output owners within each profile; retain the exact root set.
fn order_roots(roots: &mut [Root], outside_in: bool) {
    let mut start = 0;
    while start < roots.len() {
        let end = start + roots[start..].partition_point(|r| r.profile == roots[start].profile);
        let original = roots[start..end].to_vec();
        for (offset, root) in roots[start..end].iter_mut().enumerate() {
            let index = if outside_in {
                if offset % 2 == 0 {
                    offset / 2
                } else {
                    original.len() - 1 - offset / 2
                }
            } else {
                original.len() - 1 - offset
            };
            *root = original[index];
        }
        start = end;
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;

    #[test]
    fn ordering_preserves_every_owner_and_profile_for_odd_and_even_widths() {
        for width in 1..=16 {
            let original: Vec<_> = (0..3)
                .flat_map(|profile| {
                    (0..width).map(move |source| Root {
                        profile,
                        source: Some(source),
                        impossible: false,
                    })
                })
                .collect();
            for outside_in in [false, true] {
                let mut roots = original.clone();
                order_roots(&mut roots, outside_in);
                for profile in 0..3 {
                    let owners: BTreeSet<_> = roots
                        .iter()
                        .filter(|r| r.profile == profile)
                        .map(|r| r.source)
                        .collect();
                    assert_eq!(owners, (0..width).map(Some).collect());
                }
                assert_eq!(roots.len(), original.len());
            }
        }
    }
}
