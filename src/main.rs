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

#[derive(Debug, Clone)]
enum Msg {
    Copy { did: DataId, data: Arc<Data> },
    Compute { expr: Arc<Expr> },
}

trait Logger: std::fmt::Debug + Send {
    fn line_writer(&mut self) -> Option<&mut dyn Write>;
}
trait PolicyReasoner: Send {
    fn may_send_to(&mut self, did: DataId, eids: &HashSet<ExprId>, sid: SiteId) -> bool;
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
    did_to_eid: OneToManyMap<DataId, ExprId>,
    reasoner: Box<dyn PolicyReasoner>,
    logger: Box<dyn Logger>,
    network: Box<dyn Network>,
    compute_fn: fn(&[&Data]) -> Arc<Data>,
    my_sid: SiteId,
}

/////////////////////////////////////////////////////

fn sites_setup(site_log_names: &HashMap<SiteId, &'static str>) -> HashMap<SiteId, Site> {
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
            let reasoner: Box<dyn PolicyReasoner> = Box::new(PolicyReasonerImpl);
            let logger: Box<LoggerImpl> =
                Box::new(LoggerImpl::new(&format!("./logs/{}", log_name)));
            let network = Box::new(NetworkImpl::new(outboxes.clone(), inbox));
            let site = Site::new(reasoner, logger, network, compute_fn, sid);
            (sid, site)
        })
        .collect()
}

fn main() {
    let site_log_names = maplit::hashmap! {
        SiteId(Id { bits: 0 }) => "Amy",
        SiteId(Id { bits: 1 }) => "Bob",
        SiteId(Id { bits: 2 }) => "Cho",
    };
    let mut sites = sites_setup(&site_log_names);
    {
        let site = sites.get_mut(&SiteId(Id { bits: 0 })).unwrap();
        let a: Arc<Data> = (b"arg a" as &Data).into();
        let b: Arc<Data> = (b"arg b" as &Data).into();
        let f: Arc<Data> = (b"compute f" as &Data).into();

        let did_a = site.add_data(a);
        let did_b = site.add_data(b);
        let did_f = site.add_data(f);

        let expr_fab = Arc::new(Expr::ComputeWith(vec![
            Expr::Data(did_f),
            Expr::Data(did_a),
            Expr::Data(did_b),
        ]));
        let _eid_fab = site.add_expr(expr_fab);
    }
    crossbeam_utils::thread::scope(|s| {
        for site in sites.values_mut() {
            s.spawn(move |_| site.execute());
        }
    })
    .unwrap();
}
