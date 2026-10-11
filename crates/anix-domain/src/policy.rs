use crate::*;

impl HypothesisStatus {
    pub fn transition(self, next: Self) -> Result<()> {
        use HypothesisStatus::*;
        if matches!(
            (self, next),
            (Draft, Proposed | Active | Archived)
                | (Proposed, Active | Rejected | Archived)
                | (Active, Supported | Rejected | Inconclusive | Archived)
                | (Supported | Rejected | Inconclusive, Active | Archived)
        ) {
            Ok(())
        } else {
            Err(Error::InvalidTransition(format!(
                "hypothesis {self:?} -> {next:?}"
            )))
        }
    }
}
impl ExperimentStatus {
    pub fn transition(self, next: Self) -> Result<()> {
        use ExperimentStatus::*;
        if matches!(
            (self, next),
            (Draft, Proposed | Approved | Cancelled | Archived)
                | (Proposed, Approved | Cancelled | Archived)
                | (Approved, Ready | Running | Cancelled | Archived)
                | (Ready, Running | Cancelled | Archived)
                | (Running, Completed | Failed | Cancelled)
                | (Failed, Ready | Archived)
                | (Completed | Cancelled, Archived)
        ) {
            Ok(())
        } else {
            Err(Error::InvalidTransition(format!(
                "experiment {self:?} -> {next:?}"
            )))
        }
    }
}
impl RunStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
    pub fn transition(self, next: Self) -> Result<()> {
        use RunStatus::*;
        if matches!(
            (self, next),
            (Pending, Running | Cancelled)
                | (Running, WaitingForHuman | Completed | Failed | Cancelled)
                | (WaitingForHuman, Running | Completed | Failed | Cancelled)
        ) {
            Ok(())
        } else {
            Err(Error::InvalidTransition(format!(
                "run {self:?} -> {next:?}"
            )))
        }
    }
}
impl GateStatus {
    pub fn resolve(self, next: Self) -> Result<()> {
        if self != Self::Pending {
            return Err(Error::GateAlreadyResolved);
        }
        if next == Self::Pending {
            return Err(Error::InvalidTransition("gate remains pending".into()));
        }
        Ok(())
    }
}

pub fn confidence(value: Option<f64>) -> Result<()> {
    if value.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err(Error::Validation("confidence must be in [0,1]".into()));
    }
    Ok(())
}

pub fn validate_relation(from: EntityRef, to: EntityRef, kind: RelationKind) -> Result<()> {
    use RelationKind::*;
    if from == to {
        return Err(Error::InvalidRelation);
    }
    let valid = match kind {
        Supports | Contradicts | Qualifies => {
            matches!(from, EntityRef::Claim(_) | EntityRef::Finding(_))
                && matches!(to, EntityRef::Hypothesis(_))
        }
        Contains => matches!((from, to), (EntityRef::Paper(_), EntityRef::Claim(_))),
        Tests => matches!(
            (from, to),
            (EntityRef::Experiment(_), EntityRef::Hypothesis(_))
        ),
        Addresses => matches!(
            (from, to),
            (EntityRef::Hypothesis(_), EntityRef::Question(_))
        ),
        Produced => {
            matches!(from, EntityRef::ExperimentRun(_))
                && matches!(to, EntityRef::Finding(_) | EntityRef::Artifact(_))
        }
        DerivedFrom => matches!(
            (from, to),
            (EntityRef::Paper(_), EntityRef::Source(_))
                | (EntityRef::Claim(_), EntityRef::Source(_))
                | (EntityRef::Finding(_), EntityRef::ExperimentRun(_))
                | (
                    EntityRef::Artifact(_),
                    EntityRef::Source(_) | EntityRef::ExperimentRun(_)
                )
        ),
        Uses => {
            matches!(from, EntityRef::Experiment(_) | EntityRef::ExperimentRun(_))
                && matches!(to, EntityRef::Source(_) | EntityRef::Artifact(_))
        }
        Supersedes => from.kind() == to.kind(),
        BasedOn => matches!(
            from,
            EntityRef::Hypothesis(_) | EntityRef::Experiment(_) | EntityRef::Finding(_)
        ),
        References | RelatedTo => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidRelation)
    }
}

/// All agent semantic changes pass through a gate; adding a non-destructive
/// reference relation is the only automatically executable action in V1.
pub struct GatePolicy;
impl GatePolicy {
    pub fn requires_approval(action: &ProposedAction) -> bool {
        !matches!(
            action,
            ProposedAction::CreateRelation(NewRelation {
                kind: RelationKind::References | RelationKind::RelatedTo,
                ..
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transitions() {
        assert!(HypothesisStatus::Draft
            .transition(HypothesisStatus::Active)
            .is_ok());
        assert!(HypothesisStatus::Draft
            .transition(HypothesisStatus::Supported)
            .is_err());
        assert!(ExperimentStatus::Running
            .transition(ExperimentStatus::Completed)
            .is_ok());
        assert!(ExperimentStatus::Completed
            .transition(ExperimentStatus::Running)
            .is_err());
        assert!(GateStatus::Approved.resolve(GateStatus::Rejected).is_err());
        assert!(RunStatus::Failed.transition(RunStatus::Running).is_err());
    }
    #[test]
    fn ids_and_relations() {
        let id = HypothesisId::new();
        assert_eq!(id.to_string().parse::<HypothesisId>().unwrap(), id);
        assert!("bad".parse::<QuestionId>().is_err());
        assert!(validate_relation(
            EntityRef::Claim(ClaimId::new()),
            EntityRef::Hypothesis(id),
            RelationKind::Supports
        )
        .is_ok());
        assert!(validate_relation(
            EntityRef::Paper(PaperId::new()),
            EntityRef::Hypothesis(id),
            RelationKind::Supports
        )
        .is_err());
    }
    #[test]
    fn secrets() {
        assert!(safe_metadata(&serde_json::json!({"API_KEY":"value"})).is_err());
        assert!(safe_metadata(&serde_json::json!({"page":1})).is_ok());
        assert!(confidence(Some(f64::NAN)).is_err());
    }
}
