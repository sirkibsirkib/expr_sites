use super::*;

pub struct PolicyReasonerImpl;
impl PolicyReasoner for PolicyReasonerImpl {
    fn may_access(&mut self, _: DataId, _: &HashSet<ExprId>, _: SiteId) -> bool {
        true
    }
    fn may_compute(&mut self, eid: ExprId, sid: SiteId) -> bool {
        eid.0.bits ^ sid.0.bits > 2
    }
}
