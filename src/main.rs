use crate::logger::LoggerImpl;
use crate::network::NetworkImpl;
use crate::policy_reasoner::PolicyReasonerImpl;
use crossbeam_channel::{Receiver, Sender};
use one_to_many_map::OneToManyMap;
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    fs::File,
    hash::{Hash, Hasher},
    io::Write,
    path::Path,
    sync::Arc,
    time::Duration,
};

#[macro_use]
mod macros;

mod logger;
mod network;
mod policy_reasoner;
mod site;

const TIMEOUT_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
enum Expr {
    ExprId(ExprId),
    Data(DataId),
    ComputeWith(Vec<Expr>),
}

type Data = [u8];

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct Id {
    bits: u64,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct ExprId(Id);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct DataId(Id);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct SiteId(Id);

trait IdHashes {
    type Id: Sized;
    fn id_hash(&self) -> Self::Id;
}

#[derive(Debug, Clone)]
enum Msg {
    Copy { did: DataId, data: Arc<Data> },
    Compute { expr: Arc<Expr> },
}

trait Logger: std::fmt::Debug + Send {
    fn line_writer(&mut self) -> Option<&mut dyn Write>;
}
trait PolicyReasoner: Send {
    fn may_access(&mut self, did: DataId, eids: &HashSet<ExprId>, sid: SiteId) -> bool;
    fn may_compute(&mut self, eid: ExprId, sid: SiteId) -> bool;
}
trait Network: Send {
    fn send_to_where(
        &mut self,
        msg: &Msg,
        send_site_predicate: &mut dyn FnMut(SiteId) -> bool,
    ) -> Result<(), ()>;
    fn send_to(&mut self, msg: &Msg, sid: SiteId) -> Result<(), ()>;
    fn try_recv(&mut self) -> Option<Msg>;
}

fn compute_fn(args: &[&Data]) -> Arc<Data> {
    args.iter().map(|arg| arg.len() as u8).collect()
}

struct Site {
    did_to_data: HashMap<DataId, Arc<Data>>,
    eid_to_children: HashMap<ExprId, Vec<ExprId>>,
    did_to_eids: OneToManyMap<DataId, ExprId>,
    reasoner: Box<dyn PolicyReasoner>,
    logger: Box<dyn Logger>,
    network: Box<dyn Network>,
    compute_fn: fn(&[&Data]) -> Arc<Data>,
    my_sid: SiteId,
}

/////////////////////////////////////////////////////

impl IdHashes for Data {
    type Id = DataId;
    fn id_hash(&self) -> Self::Id {
        let mut h = DefaultHasher::default();
        h.write_u8(b'D');
        self.hash(&mut h);
        DataId(Id { bits: h.finish() })
    }
}

impl IdHashes for Expr {
    type Id = ExprId;
    fn id_hash(&self) -> Self::Id {
        match self {
            Expr::ExprId(eid) => *eid,
            Expr::Data(did) => {
                let mut h = DefaultHasher::default();
                h.write_u64(did.0.bits);
                h.write_u8(b'L'); // for 'leaf node'
                ExprId(Id { bits: h.finish() })
            }
            Expr::ComputeWith(child_exprs) => {
                let mut h = DefaultHasher::default();
                for child_expr in child_exprs.iter() {
                    h.write_u64(child_expr.id_hash().0.bits)
                }
                h.write_u8(b'I'); // for 'inner node'
                ExprId(Id { bits: h.finish() })
            }
        }
    }
}

fn sites_setup(
    site_log_names: &HashMap<SiteId, &'static str>,
    pri: &PolicyReasonerImpl,
) -> HashMap<SiteId, Site> {
    let mut outboxes = Arc::new(HashMap::default());
    let outboxes_ref = Arc::get_mut(&mut outboxes).unwrap();
    let mut setups = HashMap::<SiteId, SiteSetup>::default();
    struct SiteSetup {
        log_name: &'static str,
        inbox: Receiver<Msg>,
    }
    for (&sid, log_name) in site_log_names.iter() {
        let (outbox, inbox) = crossbeam_channel::unbounded();
        outboxes_ref.insert(sid, outbox);
        setups.insert(sid, SiteSetup { log_name, inbox });
    }
    setups
        .into_iter()
        .map(|(sid, SiteSetup { log_name, inbox })| {
            // ok
            let reasoner: Box<dyn PolicyReasoner> = Box::new(pri.clone());
            let logger: Box<LoggerImpl> =
                Box::new(LoggerImpl::new(&format!("./logs/{}", log_name)));
            let network = Box::new(NetworkImpl::new(outboxes.clone(), inbox));
            let site = Site::new(reasoner, logger, network, compute_fn, sid);
            (sid, site)
        })
        .collect()
}

fn main() {
    const DATAS: [&Data; 2] = [b"arg a", b"compute f"];
    const fn sid(bits: u64) -> SiteId {
        SiteId(Id { bits })
    }
    const AMY: SiteId = sid(0);
    const BOB: SiteId = sid(1);
    const CHO: SiteId = sid(2);
    let site_log_names = maplit::hashmap! {
        AMY => "Amy",
        BOB => "Bob",
        CHO => "Cho",
    };
    let did_a = DATAS[0].id_hash();
    let did_f = DATAS[0].id_hash();
    let expr_fa = Arc::new(Expr::ComputeWith(vec![Expr::Data(did_f), Expr::Data(did_a)]));
    let eid_fa = expr_fa.id_hash();

    let pri = Arc::new(PolicyReasonerImpl::new(
        Arc::new(move |_did, _eids, _sid| {
            // MAY ACCESS
            // ok
            true
        }),
        Arc::new(move |_eid, sid: SiteId| {
            // MAY COMPUTE
            sid == AMY
            // true
        }),
    ));
    let mut sites = sites_setup(&site_log_names, &pri);
    sites.get_mut(&AMY).unwrap().add_data(DATAS[0].into());
    sites.get_mut(&BOB).unwrap().add_data(DATAS[1].into());
    sites.get_mut(&CHO).unwrap().add_expr(expr_fa);

    crossbeam_utils::thread::scope(|s| {
        for (sid, site) in sites.iter_mut() {
            s.spawn(move |_| {
                if sid == &AMY {
                    loop {
                        if let Some(&did) = site.eid_to_did(&eid_fa) {
                            log!(site.logger_mut(), "AYYY did={:?}", did);
                            println!("AMY GOT IT");
                            break;
                        }
                        site.step();
                    }
                } else {
                    loop {
                        site.step()
                    }
                }
            });
        }
    })
    .unwrap();
}
