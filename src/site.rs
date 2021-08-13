use super::*;

impl Site {
    pub fn new(
        reasoner: Box<dyn PolicyReasoner>,
        logger: Box<dyn Logger>,
        network: Box<dyn Network>,
        compute_fn: fn(&[&Data]) -> Arc<Data>,
        my_sid: SiteId,
    ) -> Self {
        Self {
            did_to_data: Default::default(),
            eid_to_children: Default::default(),
            did_to_eids: Default::default(),
            reasoner,
            logger,
            network,
            compute_fn,
            my_sid,
        }
    }

    pub fn logger_mut(&mut self) -> &mut dyn Logger {
        &mut *self.logger
    }

    pub fn eid_to_did(&self, eid: &ExprId) -> Option<&DataId> {
        self.did_to_eids.get_one(eid)
    }

    // this is what the user calls
    pub fn add_data(&mut self, data: Arc<Data>) -> DataId {
        let did = data.id_hash();
        let Self { logger, did_to_data, reasoner, did_to_eids, network, my_sid, .. } = self;
        if did_to_data.contains_key(&did) {
            log!(logger, "Already got this data, thanks. Did={:?}", did);
        } else {
            let eid = Expr::Data(did).id_hash();
            did_to_eids.insert(did, eid).unwrap();
            if reasoner.may_access(did, did_to_eids.get_many(&did), *my_sid) {
                did_to_data.insert(did, data.clone());
            } else {
                log!(logger, "OOh crikey I am not allowed to have this data! did={:?}", did);
            }
            let msg = Msg::Copy { did, data };
            // ... send it to all peers who may receive it...
            let mut send_pred = |sid| {
                let do_send =
                    sid != *my_sid && reasoner.may_access(did, did_to_eids.get_many(&did), sid);
                if do_send {
                    log!(logger, "Sending to {:?} msg {:?}", sid, msg);
                }
                do_send
            };
            network.send_to_where(&msg, &mut send_pred).unwrap();
        }
        did
    }

    // This is what the user calls
    pub fn add_expr(&mut self, expr: Arc<Expr>) -> ExprId {
        // replicate this call at peer sites
        let msg = Msg::Compute { expr: expr.clone() };
        let Self { my_sid, logger, network, .. } = self;
        network
            .send_to_where(&msg, &mut |sid| {
                let do_send = sid != *my_sid;
                if do_send {
                    log!(logger, "Sending to {:?} msg {:?}", sid, msg);
                }
                do_send
            })
            .unwrap();
        // now actually do the real work
        self.add_replicated_expr(&expr)
    }

    fn add_replicated_expr(&mut self, expr: &Expr) -> ExprId {
        // recursively walk down the expr, adding it into myself.
        // NO need to send this expression to my peers since it's already been replicated via a broadcast.
        match expr {
            Expr::ExprId(eid) => *eid,
            Expr::Data(did) => {
                let mut h = DefaultHasher::default();
                h.write_u64(did.0.bits);
                h.write_u8(b'L'); // for 'leaf node'
                let eid = ExprId(Id { bits: h.finish() });
                self.did_to_eids.insert(*did, eid).unwrap();
                eid
            }
            Expr::ComputeWith(child_exprs) => {
                let mut h = DefaultHasher::default();
                let children = ExprChildren {
                    child_eids: child_exprs
                        .iter()
                        .map(|eid| {
                            let child_eid = self.add_replicated_expr(eid);
                            h.write_u64(child_eid.0.bits);
                            child_eid
                        })
                        .collect(),
                };
                h.write_u8(b'I'); // for 'inner node'
                let eid = ExprId(Id { bits: h.finish() });
                self.eid_to_children.insert(eid, children);
                eid
            }
        }
    }

    pub fn step(&mut self) {
        // let's try and compute everything we can
        let Self {
            did_to_eids, eid_to_children, did_to_data, reasoner, my_sid, compute_fn, ..
        } = self;
        for (&parent_eid, children) in eid_to_children {
            // ... whose expr is not associated with data...
            if did_to_eids.get_one(&parent_eid).is_some() {
                continue;
            }
            // ... whose inputs have known data ...
            if let Some(child_datas) = children
                .child_eids
                .iter()
                .map(|e| {
                    did_to_eids.get_one(e).and_then(|did| did_to_data.get(did).map(AsRef::as_ref))
                })
                .collect::<Option<Vec<&Data>>>()
            {
                // ... which I am allowed to compute here...
                if !reasoner.may_compute(parent_eid, *my_sid) {
                    continue;
                }
                log!(self.logger, "computing expression with Eid {:?}", parent_eid);
                // ... compute this result...
                let data = (compute_fn)(&child_datas);
                let did = self.add_data(data.clone());
                let msg = Msg::DidToEid { did, eid: parent_eid };
                let Self { network, logger, did_to_eids, my_sid, .. } = self;
                network
                    .send_to_where(&msg, &mut |sid| {
                        let do_send = sid != *my_sid;
                        if do_send {
                            log!(logger, "Sending to {:?} msg {:?}", sid, msg);
                        }
                        do_send
                    })
                    .unwrap();
                did_to_eids.insert(did, parent_eid).unwrap();

                log!(logger, "Result has did={:?} and data={:?}", did, &data);
                // // Let's reconsider all compute steps. New things may be possible
                return;
            }
        }

        // handle all ready messages
        let mut recvd_one = false;
        while let Some(msg) = self.network.try_recv() {
            log!(self.logger, "Received some msg {:?}", msg);
            recvd_one = true;
            match msg {
                Msg::Compute { expr } => {
                    self.add_replicated_expr(&expr);
                }
                Msg::DidToEid { did, eid } => {
                    self.did_to_eids.insert(did, eid).unwrap();
                }
                Msg::Copy { did, data } => {
                    self.did_to_data.insert(did, data);
                }
            }
        }
        if !recvd_one {
            log!(self.logger, "Taking a little sleep");
            // let's take a breather before we continue working. don't want to spinlock!
            std::thread::sleep(TIMEOUT_DURATION);
        }
    }
}
