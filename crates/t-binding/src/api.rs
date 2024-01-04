use crate::{JSEngine, Runner};
pub trait Callback<Args> {
    fn call(&self, runner: &mut Runner, args: Args);
}

impl<F, P1> Callback<(P1,)> for F
where
    P1: Clone + 'static + Send + Sync,
    F: Fn(&mut Runner, P1),
{
    fn call(&self, runner: &mut Runner, args: (P1,)) {
        self(runner, args.0)
    }
}

impl<F, P1, P2> Callback<(P1, P2)> for F
where
    P1: Clone + 'static + Send + Sync,
    P2: Clone + 'static + Send + Sync,
    F: Fn(&mut Runner, P1, P2),
{
    fn call(&self, runner: &mut Runner, args: (P1, P2)) {
        self(runner, args.0, args.1)
    }
}

// macro_rules! impl_handler {
//     ($( $P:ident ),*) => {
//         impl<F, $($P,)*> SyncHandler<($($P,)*)> for F
//         where
//             $( $P: Clone + 'static + Send + Sync, )*
//             F: Fn($($P,)*),
//         {
//             fn call(&self) {
//                 self($(_e.get_data::<$P>(),)*);
//             }
//         }
//     };
// }

// impl_handler!();
// impl_handler!(P1);
// impl_handler!(P1, P2);
// impl_handler!(P1, P2, P3);
// impl_handler!(P1, P2, P3, P4);
// impl_handler!(P1, P2, P3, P4, P5);
// impl_handler!(P1, P2, P3, P4, P5, P6);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7, P8);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7, P8, P9);

pub fn type_string(runner: &mut Runner, s: &str) {
    todo!()
}

pub fn assert_screen(runner: &mut Runner, tags: Vec<String>) {
    todo!()
}

pub fn assert_script_run(runner: &mut Runner, cmd: String, timeout: u32) {
    todo!()
}
