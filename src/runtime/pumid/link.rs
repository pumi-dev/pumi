use crate::runtime::Error;
use serde_json::Value;
#[derive(Debug, Clone, PartialEq)]
pub enum Link<C> {
    Idle,
    Updating { generation: u64 },
    Opening { generation: u64 },
    Unanswered { generation: u64, reason: Error },
    Starting { generation: u64, reason: Error },
    Introducing { conn: C, generation: u64 },
    Attaching { conn: C, generation: u64 },
    Registering { conn: C, client: u64, generation: u64 },
    Attached { conn: C, client: u64, generation: u64 },
    Lost { generation: u64, reason: Error },
}
#[derive(Debug, Clone, PartialEq)]
pub enum Input<C> {
    Connect,
    Updated,
    Opened(C),
    OpenFailed(Error),
    Introduced,
    Attached { client: u64, state: Option<Value> },
    AttachFailed(Error),
    Registered,
    Lost { generation: u64, reason: Error },
    Start,
    Started,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Effect<C> {
    CheckUpdate,
    Open { generation: u64 },
    Introduce(C),
    Attach(C),
    KeepModel(Option<Value>),
    Register(C),
    ForgetModel,
    Wake,
    StartServer,
}
impl<C: Clone> Link<C> {
    pub fn generation(&self) -> u64 {
        match self {
            Link::Idle => 0,
            Link::Updating { generation }
            | Link::Opening { generation }
            | Link::Unanswered { generation, .. }
            | Link::Starting { generation, .. }
            | Link::Introducing { generation, .. }
            | Link::Attaching { generation, .. }
            | Link::Registering { generation, .. }
            | Link::Attached { generation, .. }
            | Link::Lost { generation, .. } => *generation,
        }
    }
    pub fn client(&self) -> Option<u64> {
        match self {
            Link::Registering { client, .. } | Link::Attached { client, .. } => Some(*client),
            _ => None,
        }
    }
    pub fn conn(&self) -> Option<&C> {
        match self {
            Link::Attached { conn, .. } => Some(conn),
            _ => None,
        }
    }
    fn socket(&self) -> Option<u64> {
        match self {
            Link::Introducing { generation, .. }
            | Link::Attaching { generation, .. }
            | Link::Registering { generation, .. } => Some(generation + 1),
            Link::Attached { generation, .. } => Some(*generation),
            _ => None,
        }
    }
    pub fn next(self, input: Input<C>) -> (Link<C>, Vec<Effect<C>>) {
        let generation = self.generation();
        match (self, input) {
            (Link::Idle | Link::Unanswered { .. } | Link::Starting { .. } | Link::Lost { .. }, Input::Connect) => {
                (Link::Updating { generation }, vec![Effect::CheckUpdate])
            }
            (Link::Updating { generation }, Input::Updated) => {
                (Link::Opening { generation }, vec![Effect::Open { generation: generation + 1 }])
            }
            (Link::Opening { generation }, Input::Opened(conn)) => {
                (Link::Introducing { conn: conn.clone(), generation }, vec![Effect::Introduce(conn)])
            }
            (Link::Opening { generation }, Input::OpenFailed(reason)) => {
                (Link::Unanswered { generation, reason }, vec![])
            }
            (Link::Introducing { conn, generation }, Input::Introduced) => {
                (Link::Attaching { conn: conn.clone(), generation }, vec![Effect::Attach(conn)])
            }
            (
                Link::Introducing { generation, .. } | Link::Attaching { generation, .. },
                Input::AttachFailed(reason),
            ) => (Link::Lost { generation, reason }, vec![]),
            (Link::Attaching { conn, generation }, Input::Attached { client, state }) => (
                Link::Registering { conn: conn.clone(), client, generation },
                vec![Effect::KeepModel(state), Effect::Register(conn)],
            ),
            (Link::Registering { conn, client, generation }, Input::Registered) => {
                (Link::Attached { conn, client, generation: generation + 1 }, vec![])
            }
            (link, Input::Lost { generation: ended, reason }) if link.socket() == Some(ended) => {
                (Link::Lost { generation, reason }, vec![Effect::ForgetModel, Effect::Wake])
            }
            (Link::Unanswered { generation, reason } | Link::Lost { generation, reason }, Input::Start) => {
                (Link::Starting { generation, reason }, vec![Effect::StartServer])
            }
            (Link::Starting { generation, reason }, Input::Started) => {
                (Link::Unanswered { generation, reason }, vec![])
            }
            (link, _) => (link, vec![]),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    type Sock = &'static str;
    fn gone() -> Error {
        Error::Unreachable("the server went away".into())
    }
    fn run(mut link: Link<Sock>, inputs: Vec<Input<Sock>>) -> (Link<Sock>, Vec<Effect<Sock>>) {
        let mut all = Vec::new();
        for input in inputs {
            let (next, effects) = link.next(input);
            link = next;
            all.extend(effects);
        }
        (link, all)
    }
    fn attach(link: Link<Sock>, sock: Sock, client: u64) -> (Link<Sock>, Vec<Effect<Sock>>) {
        run(
            link,
            vec![
                Input::Connect,
                Input::Updated,
                Input::Opened(sock),
                Input::Introduced,
                Input::Attached { client, state: Some(json!({ "sessions": [] })) },
                Input::Registered,
            ],
        )
    }
    #[test]
    fn a_fresh_attach_checks_opens_introduces_attaches_and_registers_in_order() {
        let (link, effects) = attach(Link::Idle, "a", 7);
        assert_eq!(link, Link::Attached { conn: "a", client: 7, generation: 1 });
        assert_eq!(
            effects,
            vec![
                Effect::CheckUpdate,
                Effect::Open { generation: 1 },
                Effect::Introduce("a"),
                Effect::Attach("a"),
                Effect::KeepModel(Some(json!({ "sessions": [] }))),
                Effect::Register("a"),
            ]
        );
        assert_eq!((link.generation(), link.client(), link.conn()), (1, Some(7), Some(&"a")));
    }
    #[test]
    fn the_generation_moves_only_once_registered() {
        let (link, _) = run(
            Link::Idle,
            vec![
                Input::Connect,
                Input::Updated,
                Input::Opened("a"),
                Input::Introduced,
                Input::Attached { client: 3, state: None },
            ],
        );
        assert_eq!(link, Link::Registering { conn: "a", client: 3, generation: 0 });
        assert!(link.conn().is_none());
        assert_eq!(link.client(), Some(3));
    }
    #[test]
    fn a_loss_then_a_reconnect_is_a_new_generation_that_registers_again() {
        let (link, _) = attach(Link::Idle, "a", 7);
        let (link, effects) = link.next(Input::Lost { generation: 1, reason: gone() });
        assert_eq!(link, Link::Lost { generation: 1, reason: gone() });
        assert_eq!(effects, vec![Effect::ForgetModel, Effect::Wake]);
        assert_eq!(link.generation(), 1, "a loss keeps the count");
        let (link, effects) = attach(link, "b", 9);
        assert_eq!(link, Link::Attached { conn: "b", client: 9, generation: 2 });
        assert!(effects.contains(&Effect::Open { generation: 2 }));
        assert!(effects.contains(&Effect::Register("b")), "{effects:?}");
    }
    #[test]
    fn a_failed_open_is_unanswered_and_the_setup_may_start_the_server() {
        let (link, effects) = run(Link::Idle, vec![Input::Connect, Input::Updated, Input::OpenFailed(gone())]);
        assert_eq!(link, Link::Unanswered { generation: 0, reason: gone() });
        assert_eq!(effects, vec![Effect::CheckUpdate, Effect::Open { generation: 1 }]);
        let (link, effects) = link.next(Input::Start);
        assert_eq!(link, Link::Starting { generation: 0, reason: gone() });
        assert_eq!(effects, vec![Effect::StartServer]);
        let (link, effects) = link.clone().next(Input::Started);
        assert_eq!((link.clone(), effects), (Link::Unanswered { generation: 0, reason: gone() }, vec![]));
        let (link, _) = attach(link, "a", 1);
        assert_eq!(link.generation(), 1);
    }
    #[test]
    fn connecting_while_the_server_starts_goes_ahead_and_the_start_ending_changes_nothing() {
        let starting = Link::<Sock>::Starting { generation: 0, reason: gone() };
        let (link, _) = attach(starting, "a", 2);
        assert_eq!(link, Link::Attached { conn: "a", client: 2, generation: 1 });
        assert_eq!(link.clone().next(Input::Started), (link, vec![]));
    }
    #[test]
    fn a_refused_attach_is_lost_without_a_new_generation() {
        let refused = Error::Refused("no".into());
        let (link, effects) = run(
            Link::Idle,
            vec![Input::Connect, Input::Updated, Input::Opened("a"), Input::AttachFailed(refused.clone())],
        );
        assert_eq!(link, Link::Lost { generation: 0, reason: refused });
        assert_eq!(effects.last(), Some(&Effect::Introduce("a")));
    }
    #[test]
    fn a_socket_lost_while_attaching_forgets_and_its_late_answers_are_ignored() {
        let (link, _) = run(Link::Idle, vec![Input::Connect, Input::Updated, Input::Opened("a")]);
        let (link, effects) = link.next(Input::Lost { generation: 1, reason: gone() });
        assert_eq!(link, Link::Lost { generation: 0, reason: gone() });
        assert_eq!(effects, vec![Effect::ForgetModel, Effect::Wake]);
        let (link, effects) =
            run(link, vec![Input::Introduced, Input::Attached { client: 1, state: None }, Input::Registered]);
        assert_eq!((link, effects), (Link::Lost { generation: 0, reason: gone() }, vec![]));
    }
    #[test]
    fn inputs_impossible_in_a_phase_are_ignored() {
        let (attached, _) = attach(Link::Idle, "a", 7);
        let lost = Link::<Sock>::Lost { generation: 1, reason: gone() };
        let cases: Vec<(Link<Sock>, Input<Sock>)> = vec![
            (Link::Idle, Input::Updated),
            (Link::Idle, Input::Opened("x")),
            (Link::Idle, Input::Registered),
            (Link::Idle, Input::Lost { generation: 0, reason: gone() }),
            (Link::Idle, Input::Start),
            (Link::Idle, Input::Started),
            (attached.clone(), Input::Connect),
            (attached.clone(), Input::Attached { client: 1, state: None }),
            (attached.clone(), Input::AttachFailed(gone())),
            (attached.clone(), Input::Start),
            (attached.clone(), Input::Lost { generation: 0, reason: gone() }),
            (Link::Updating { generation: 0 }, Input::Connect),
            (Link::Updating { generation: 0 }, Input::Opened("x")),
            (Link::Opening { generation: 0 }, Input::Introduced),
            (Link::Opening { generation: 0 }, Input::Lost { generation: 1, reason: gone() }),
            (Link::Introducing { conn: "a", generation: 0 }, Input::Registered),
            (Link::Attaching { conn: "a", generation: 0 }, Input::Introduced),
            (Link::Registering { conn: "a", client: 1, generation: 0 }, Input::AttachFailed(gone())),
            (lost.clone(), Input::Lost { generation: 1, reason: gone() }),
            (lost.clone(), Input::Registered),
            (lost, Input::Started),
            (Link::Starting { generation: 0, reason: gone() }, Input::Start),
        ];
        for (link, input) in cases {
            let what = format!("{input:?} in {link:?}");
            assert_eq!(link.clone().next(input), (link, vec![]), "{what}");
        }
    }
}
