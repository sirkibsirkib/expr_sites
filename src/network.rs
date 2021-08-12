use super::*;
use std::sync::Arc;

pub(crate) struct NetworkImpl {
    outboxes: Arc<HashMap<SiteId, Sender<Msg>>>,
    inbox: Receiver<Msg>,
}
impl NetworkImpl {
    pub fn new(outboxes: Arc<HashMap<SiteId, Sender<Msg>>>, inbox: Receiver<Msg>) -> Self {
        Self { outboxes, inbox }
    }
}
impl Network for NetworkImpl {
    fn send_to_where(
        &mut self,
        msg: &Msg,
        send_site_predicate: &mut dyn FnMut(SiteId) -> bool,
    ) -> Result<(), ()> {
        for (&sid, outbox) in self.outboxes.iter() {
            if send_site_predicate(sid) {
                outbox.send(msg.clone()).map_err(drop)?
            }
        }
        Ok(())
    }
    fn send_to(&mut self, msg: &Msg, sid: SiteId) -> Result<(), ()> {
        self.outboxes.get(&sid).unwrap().send(msg.clone()).map_err(drop)
    }
    fn try_recv(&mut self) -> Option<Msg> {
        self.inbox.try_recv().ok()
    }
}
