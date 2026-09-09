/// Children partition the parent by the second output's unique producer.
/// Only children enter the ledger; all must finish before the profile is exhausted.
fn refine_roots(
    roots: Vec<Root>,
    tasks: &[AccountedProfile],
    inputs: usize,
    outputs: usize,
    workers: usize,
) -> Vec<Root> {
    let live = roots.iter().filter(|root| !root.impossible).count();
    if workers <= 1 || outputs < 2 || live >= workers {
        return roots;
    }
    let mut refined: Vec<_> = roots
        .into_iter()
        .flat_map(|root| {
            let sources = if root.impossible || root.source.is_none() {
                0
            } else {
                inputs + tasks[root.profile].profile.node_count() as usize
            };
            if sources == 0 {
                vec![root]
            } else {
                (0..sources)
                    .map(|source| Root {
                        second_source: Some(source),
                        ..root
                    })
                    .collect()
            }
        })
        .collect();
    refined.sort_by_key(|root| root.second_source);
    refined
}
