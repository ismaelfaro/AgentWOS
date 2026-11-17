//! The built-in MCP guard: deterministic checks on everything that crosses an
//! MCP connection. The host compiles this component into the kernel and
//! attaches it to every MCP connection, so no MCP message goes unchecked.

mod checks;

#[cfg(target_arch = "wasm32")]
mod component {
    wit_bindgen::generate!({
        world: "control-module",
        path: "../../wit/world.wit",
    });

    use exports::agentwos::core::control::{Guest, Verdict};

    struct Component;

    impl Guest for Component {
        fn inspect(_author: String, text: String, context: String) -> Verdict {
            match super::checks::inspect(&text, &context) {
                super::checks::Outcome::Allow => Verdict::Allow,
                super::checks::Outcome::Modify(t) => Verdict::Modify(t),
                super::checks::Outcome::Block(why) => Verdict::Block(why),
                super::checks::Outcome::Flag(why) => Verdict::Flag(why),
            }
        }

        fn describe() -> String {
            super::checks::describe()
        }
    }

    export!(Component);
}
