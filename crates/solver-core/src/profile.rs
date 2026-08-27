use std::collections::BTreeMap;

use solver_api::{NodeProfile, Rational};
use thiserror::Error;

/// Profiles with one exact operator-to-operator link count, ordered deterministically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileGroup {
    pub link_count: u32,
    pub profiles: Vec<NodeProfile>,
}

/// Exact operator-link and physical-link accounting for one fixed node profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileLinkAccounting {
    /// Belts whose producer and consumer are both physical operators.
    pub link_count: u32,
    /// Anonymous discard links excluded from the structural group count.
    pub discard_link_count: u32,
    /// All physical belts, including terminal stubs and discard lines.
    pub physical_link_count: u32,
}

/// One fixed profile paired with its exact discard/link accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountedProfile {
    pub profile: NodeProfile,
    pub accounting: ProfileLinkAccounting,
}

/// Profiles sharing one exact operator-to-operator link count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountedProfileGroup {
    pub link_count: u32,
    pub profiles: Vec<AccountedProfile>,
}

/// Checked-arithmetic failure while deriving profile port or link counts.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ProfileArithmeticError {
    #[error("profile node count overflowed")]
    NodeCountOverflow,
    #[error("profile producer port count overflowed")]
    ProducerPortCountOverflow,
    #[error("profile consumer port count overflowed")]
    ConsumerPortCountOverflow,
    #[error("profile physical link count overflowed")]
    LinkCountOverflow,
    #[error("profile accounting requires a nonnegative surplus")]
    NegativeSurplus,
    #[error("profile accounting requires a positive physical-link capacity")]
    NonPositiveCapacity,
}

/// Enumerates all profiles feasible under the exact surplus/discard port count
/// and the mandatory capacity of each discard link.
///
/// Equal optimized `L` obligations remain in one group even when their physical
/// discard counts differ. This is required because discard links never
/// participate in the `(node_count, link_count)` objective.
///
/// # Errors
///
/// Returns [`ProfileArithmeticError`] when a derived public count overflows,
/// the supplied surplus is negative, or the physical-link capacity is not
/// strictly positive.
pub fn enumerate_accounted_profile_groups(
    node_count: u32,
    input_count: u32,
    output_count: u32,
    surplus: &Rational,
    max_link_rate: &Rational,
) -> Result<Vec<AccountedProfileGroup>, ProfileArithmeticError> {
    let mut groups = BTreeMap::<u32, Vec<AccountedProfile>>::new();
    for splitter2 in 0..=node_count {
        let non_splitter2 = node_count - splitter2;
        for splitter3 in 0..=non_splitter2 {
            let merger_total = non_splitter2 - splitter3;
            for merger2 in 0..=merger_total {
                let profile = NodeProfile {
                    splitter2,
                    splitter3,
                    merger2,
                    merger3: merger_total - merger2,
                };
                for accounting in profile_link_accountings(
                    profile,
                    input_count,
                    output_count,
                    surplus,
                    max_link_rate,
                )? {
                    groups
                        .entry(accounting.link_count)
                        .or_default()
                        .push(AccountedProfile {
                            profile,
                            accounting,
                        });
                }
            }
        }
    }
    Ok(groups
        .into_iter()
        .map(|(link_count, mut profiles)| {
            profiles.sort_unstable_by_key(|candidate| {
                (candidate.profile, candidate.accounting.discard_link_count)
            });
            AccountedProfileGroup {
                link_count,
                profiles,
            }
        })
        .collect())
}

/// Enumerates every exact operator-link count admitted by one fixed profile.
///
/// With `P` producer ports and `C` modeled consumer ports, zero surplus admits
/// exactly `P=C`. Positive surplus requires `D=P-C>0`; `surplus<=D*B` is the
/// exact necessary and sufficient condition for splitting that surplus among
/// `D` strictly-positive discard links of capacity `B`.
///
/// # Errors
///
/// Returns [`ProfileArithmeticError`] when a derived public count overflows,
/// the supplied surplus is negative, or the physical-link capacity is not
/// strictly positive.
pub fn profile_link_accountings(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    surplus: &Rational,
    max_link_rate: &Rational,
) -> Result<Vec<ProfileLinkAccounting>, ProfileArithmeticError> {
    if surplus.is_negative() {
        return Err(ProfileArithmeticError::NegativeSurplus);
    }
    if !max_link_rate.is_positive() {
        return Err(ProfileArithmeticError::NonPositiveCapacity);
    }
    checked_node_count(profile)?;
    let physical_link_count = input_count
        .checked_add(producer_port_count(profile)?)
        .ok_or(ProfileArithmeticError::LinkCountOverflow)?;
    let link_count = output_count
        .checked_add(consumer_port_count(profile)?)
        .ok_or(ProfileArithmeticError::LinkCountOverflow)?;

    if surplus.is_zero() && physical_link_count != link_count {
        return Ok(Vec::new());
    }
    let Some(discard_link_count) = physical_link_count.checked_sub(link_count) else {
        return Ok(Vec::new());
    };
    if !surplus.is_zero()
        && (discard_link_count == 0
            || surplus > &(max_link_rate * &Rational::from(discard_link_count)))
    {
        return Ok(Vec::new());
    }
    let node_producers = producer_port_count(profile)?;
    let node_consumers = consumer_port_count(profile)?;
    let external_consumers = output_count
        .checked_add(discard_link_count)
        .ok_or(ProfileArithmeticError::LinkCountOverflow)?;
    let minimum = node_consumers
        .saturating_sub(input_count)
        .max(node_producers.saturating_sub(external_consumers));
    let maximum = node_producers.min(node_consumers);
    Ok((minimum..=maximum)
        .map(|operator_links| ProfileLinkAccounting {
            link_count: operator_links,
            discard_link_count,
            physical_link_count,
        })
        .collect())
}

/// Returns the smallest operator-link obligation admitted by one fixed profile.
///
/// Call [`profile_link_accountings`] when every exact `L` obligation is required.
///
/// # Errors
///
/// Returns [`ProfileArithmeticError`] if a public `u32` physical count cannot represent an exact
/// derived count.
pub fn profile_link_accounting(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    surplus: &Rational,
    max_link_rate: &Rational,
) -> Result<Option<ProfileLinkAccounting>, ProfileArithmeticError> {
    Ok(
        profile_link_accountings(profile, input_count, output_count, surplus, max_link_rate)?
            .into_iter()
            .next(),
    )
}

/// Enumerates every port-balanced profile for one fixed physical node count.
///
/// The returned groups use ascending exact link count. Profiles within a group use the stable
/// [`NodeProfile`] field order. No topology or arithmetic pruning occurs here.
///
/// # Errors
///
/// Returns [`ProfileArithmeticError`] if a public `u32` physical count cannot represent an exact
/// derived count.
pub fn enumerate_profile_groups(
    node_count: u32,
    input_count: u32,
    output_count: u32,
) -> Result<Vec<ProfileGroup>, ProfileArithmeticError> {
    let mut groups = BTreeMap::<u32, Vec<NodeProfile>>::new();
    for splitter2 in 0..=node_count {
        let non_splitter2 = node_count - splitter2;
        for splitter3 in 0..=non_splitter2 {
            let merger_total = non_splitter2 - splitter3;
            for merger2 in 0..=merger_total {
                let profile = NodeProfile {
                    splitter2,
                    splitter3,
                    merger2,
                    merger3: merger_total - merger2,
                };
                for accounting in profile_link_accountings(
                    profile,
                    input_count,
                    output_count,
                    &Rational::zero(),
                    &Rational::one(),
                )? {
                    groups
                        .entry(accounting.link_count)
                        .or_default()
                        .push(profile);
                }
            }
        }
    }

    Ok(groups
        .into_iter()
        .map(|(link_count, mut profiles)| {
            profiles.sort_unstable();
            ProfileGroup {
                link_count,
                profiles,
            }
        })
        .collect())
}

/// Returns the smallest exact operator-link count for a balanced profile.
///
/// # Errors
///
/// Returns [`ProfileArithmeticError`] if a derived public count overflows `u32`.
pub fn balanced_link_count(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
) -> Result<Option<u32>, ProfileArithmeticError> {
    Ok(profile_link_accounting(
        profile,
        input_count,
        output_count,
        &Rational::zero(),
        &Rational::one(),
    )?
    .map(|accounting| accounting.link_count))
}

fn checked_node_count(profile: NodeProfile) -> Result<u32, ProfileArithmeticError> {
    profile
        .splitter2
        .checked_add(profile.splitter3)
        .and_then(|count| count.checked_add(profile.merger2))
        .and_then(|count| count.checked_add(profile.merger3))
        .ok_or(ProfileArithmeticError::NodeCountOverflow)
}

fn producer_port_count(profile: NodeProfile) -> Result<u32, ProfileArithmeticError> {
    profile
        .splitter2
        .checked_mul(2)
        .and_then(|count| {
            profile
                .splitter3
                .checked_mul(3)
                .and_then(|ports| count.checked_add(ports))
        })
        .and_then(|count| count.checked_add(profile.merger2))
        .and_then(|count| count.checked_add(profile.merger3))
        .ok_or(ProfileArithmeticError::ProducerPortCountOverflow)
}

fn consumer_port_count(profile: NodeProfile) -> Result<u32, ProfileArithmeticError> {
    profile
        .splitter2
        .checked_add(profile.splitter3)
        .and_then(|count| {
            profile
                .merger2
                .checked_mul(2)
                .and_then(|ports| count.checked_add(ports))
        })
        .and_then(|count| {
            profile
                .merger3
                .checked_mul(3)
                .and_then(|ports| count.checked_add(ports))
        })
        .ok_or(ProfileArithmeticError::ConsumerPortCountOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(splitter2: u32, splitter3: u32, merger2: u32, merger3: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3,
            merger2,
            merger3,
        }
    }

    #[test]
    fn balanced_profiles_are_grouped_by_ascending_exact_link_count() {
        assert_eq!(
            enumerate_profile_groups(3, 1, 2).unwrap(),
            vec![
                ProfileGroup {
                    link_count: 3,
                    profiles: vec![profile(2, 0, 1, 0)],
                },
                ProfileGroup {
                    link_count: 4,
                    profiles: vec![profile(1, 1, 0, 1), profile(2, 0, 1, 0)],
                },
                ProfileGroup {
                    link_count: 5,
                    profiles: vec![profile(1, 1, 0, 1)],
                },
            ]
        );
        assert_eq!(
            enumerate_profile_groups(0, 1, 1).unwrap(),
            vec![ProfileGroup {
                link_count: 0,
                profiles: vec![NodeProfile::default()],
            }]
        );
        assert!(enumerate_profile_groups(0, 1, 2).unwrap().is_empty());
    }

    #[test]
    fn derived_counts_never_wrap() {
        let oversized = profile(u32::MAX, 1, 0, 0);
        assert_eq!(
            balanced_link_count(oversized, 1, 1),
            Err(ProfileArithmeticError::NodeCountOverflow)
        );

        let too_many_splitter_ports = profile(0, u32::MAX, 0, 0);
        assert_eq!(
            balanced_link_count(too_many_splitter_ports, 0, 0),
            Err(ProfileArithmeticError::ProducerPortCountOverflow)
        );

        let link_overflow = profile(0, 0, 1, 0);
        assert_eq!(
            balanced_link_count(link_overflow, u32::MAX, u32::MAX),
            Err(ProfileArithmeticError::LinkCountOverflow)
        );
    }

    #[test]
    fn exact_surplus_requires_and_accounts_for_anonymous_discard_links() {
        assert_eq!(
            profile_link_accounting(
                NodeProfile::default(),
                2,
                1,
                &"1".parse().unwrap(),
                &"1".parse().unwrap(),
            )
            .unwrap(),
            Some(ProfileLinkAccounting {
                link_count: 0,
                discard_link_count: 1,
                physical_link_count: 2,
            })
        );
        assert_eq!(
            profile_link_accounting(
                NodeProfile::default(),
                2,
                1,
                &Rational::zero(),
                &Rational::one(),
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn discard_capacity_selects_the_minimum_feasible_port_surplus() {
        let surplus = "3/2".parse().unwrap();
        let capacity = Rational::one();
        assert_eq!(
            profile_link_accounting(profile(1, 0, 0, 0), 1, 1, &surplus, &capacity).unwrap(),
            None,
            "one discard line cannot carry three halves at unit capacity"
        );
        assert_eq!(
            profile_link_accounting(profile(0, 1, 0, 0), 1, 1, &surplus, &capacity).unwrap(),
            Some(ProfileLinkAccounting {
                link_count: 0,
                discard_link_count: 2,
                physical_link_count: 4,
            })
        );
    }

    #[test]
    fn accounted_groups_are_ordered_by_exact_operator_links() {
        let groups =
            enumerate_accounted_profile_groups(1, 1, 1, &"3/2".parse().unwrap(), &Rational::one())
                .unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].link_count, 0);
        assert_eq!(groups[0].profiles.len(), 1);
        assert_eq!(groups[0].profiles[0].profile, profile(0, 1, 0, 0));
        assert_eq!(groups[0].profiles[0].accounting.discard_link_count, 2);

        let equal_link_group =
            enumerate_accounted_profile_groups(1, 1, 1, &"1/2".parse().unwrap(), &Rational::one())
                .unwrap();
        assert_eq!(equal_link_group.len(), 2);
        assert_eq!(equal_link_group[0].link_count, 0);
        let mut discard_counts = equal_link_group[0]
            .profiles
            .iter()
            .map(|candidate| candidate.accounting.discard_link_count)
            .collect::<Vec<_>>();
        discard_counts.sort_unstable();
        assert_eq!(
            discard_counts,
            vec![1, 2],
            "different physical discard counts remain one equal-L proof obligation"
        );
    }
}
