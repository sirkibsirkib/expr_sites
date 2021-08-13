use super::*;

#[derive(Clone)]
pub(crate) struct PolicyReasonerImpl {
    may_access_box: Arc<dyn Fn(DataId, &HashSet<ExprId>, SiteId) -> bool + Send + Sync + 'static>,
    may_compute_box: Arc<dyn Fn(ExprId, SiteId) -> bool + Send + Sync + 'static>,
}
impl PolicyReasonerImpl {
    pub fn new(
        may_access_box: Arc<
            dyn Fn(DataId, &HashSet<ExprId>, SiteId) -> bool + Send + Sync + 'static,
        >,
        may_compute_box: Arc<dyn Fn(ExprId, SiteId) -> bool + Send + Sync + 'static>,
    ) -> Self {
        Self { may_access_box, may_compute_box }
    }
}
impl PolicyReasoner for PolicyReasonerImpl {
    fn may_access(&mut self, did: DataId, eids: &HashSet<ExprId>, sid: SiteId) -> bool {
        (self.may_access_box)(did, eids, sid)
    }
    fn may_compute(&mut self, eid: ExprId, sid: SiteId) -> bool {
        (self.may_compute_box)(eid, sid)
    }
}
