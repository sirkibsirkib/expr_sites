use super::*;

pub struct PolicyReasonerImpl;
impl PolicyReasoner for PolicyReasonerImpl {
    fn may_send_to(&mut self, _: DataId, _: &HashSet<ExprId>, _: SiteId) -> bool {
        true
    }
    fn may_compute(&mut self, _: ExprId, _: SiteId) -> bool {
        true
    }
}
