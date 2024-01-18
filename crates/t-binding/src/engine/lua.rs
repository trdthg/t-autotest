use crate::ScriptEngine;

pub struct LuaEngine {}

impl ScriptEngine for LuaEngine {
    fn run(&mut self, _content: &str) {
        unimplemented!()
    }
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaEngine {
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_mlua() {
        let lua = mlua::Lua::new();

        let map_table = lua.create_table().unwrap();
        map_table.set(1, "one").unwrap();
        map_table.set("two", 2).unwrap();
        lua.globals().set("map_table", map_table).unwrap();

        let hello_str: &'static str = "hello";
        let hello = move || {
            println!("{}", hello_str);
        };
        lua.globals()
            .set(
                "rust_add",
                lua.create_function(move |_, (a, b): (i32, i32)| -> mlua::Result<i32> {
                    hello();
                    Ok(a + b)
                })
                .unwrap(),
            )
            .unwrap();

        let res = lua
            .load(
                r"
                for k,v in pairs(map_table) do print(k,v) end
                return rust_add(1, 2)
            ",
            )
            .eval::<i32>()
            .unwrap();
        assert!(res == 3);
    }
}
