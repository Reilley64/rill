//! PROTOTYPE (ci-ajns): emit a native object whose `main(argc, argv)` prints `Hello <argv[1]>\n`.
//! Throwaway code that answers "does inkwell on static LLVM 23 work"; not the backend.

use inkwell::AddressSpace;
use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use std::path::Path;

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: llvm23-inkwell <out.o>");

    Target::initialize_native(&InitializationConfig::default()).expect("native target init");
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).expect("target for host triple");
    let machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            // PIC: the host cc links PIE by default.
            RelocMode::PIC,
            CodeModel::Default,
        )
        .expect("target machine");

    let context = Context::create();
    let module = context.create_module("hello");
    module.set_triple(&triple);
    module.set_data_layout(&machine.get_target_data().get_data_layout());

    let builder = context.create_builder();
    let i32_type = context.i32_type();
    let i64_type = context.i64_type();
    let i8_type = context.i8_type();
    let ptr_type = context.ptr_type(AddressSpace::default());

    // libc: size_t strlen(ptr), ptr memcpy(ptr, ptr, size_t), ssize_t write(int, ptr, size_t).
    let strlen = module.add_function("strlen", i64_type.fn_type(&[ptr_type.into()], false), None);
    let memcpy = module.add_function(
        "memcpy",
        ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into()], false),
        None,
    );
    let write = module.add_function(
        "write",
        i64_type.fn_type(&[i32_type.into(), ptr_type.into(), i64_type.into()], false),
        None,
    );

    // int main(int argc, char **argv):
    //   argc < 2: write "Hello\n"
    //   else:     join "Hello " + argv[1] + "\n" in a stack buffer and write it.
    let main = module.add_function(
        "main",
        i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false),
        None,
    );
    let entry = context.append_basic_block(main, "entry");
    let bare = context.append_basic_block(main, "bare");
    let named = context.append_basic_block(main, "named");
    let argc = main.get_nth_param(0).unwrap().into_int_value();
    let argv = main.get_nth_param(1).unwrap().into_pointer_value();
    let one = i32_type.const_int(1, false);
    let zero = i32_type.const_zero();

    builder.position_at_end(entry);
    let has_name = builder
        .build_int_compare(inkwell::IntPredicate::SGT, argc, one, "has_name")
        .unwrap();
    builder
        .build_conditional_branch(has_name, named, bare)
        .unwrap();

    builder.position_at_end(bare);
    let hello_nl = builder
        .build_global_string_ptr("Hello\n", "bare_text")
        .unwrap();
    builder
        .build_call(
            write,
            &[
                one.into(),
                hello_nl.as_pointer_value().into(),
                i64_type.const_int(6, false).into(),
            ],
            "",
        )
        .unwrap();
    builder.build_return(Some(&zero)).unwrap();

    builder.position_at_end(named);
    let prefix = builder.build_global_string_ptr("Hello ", "prefix").unwrap();
    let arg_slot =
        unsafe { builder.build_gep(ptr_type, argv, &[i64_type.const_int(1, false)], "arg_slot") }
            .unwrap();
    let name = builder
        .build_load(ptr_type, arg_slot, "name")
        .unwrap()
        .into_pointer_value();
    let name_len = builder
        .build_call(strlen, &[name.into()], "name_len")
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic()
        .into_int_value();
    let total = builder
        .build_int_add(name_len, i64_type.const_int(7, false), "total")
        .unwrap();
    let buf = builder.build_array_alloca(i8_type, total, "buf").unwrap();
    builder
        .build_call(
            memcpy,
            &[
                buf.into(),
                prefix.as_pointer_value().into(),
                i64_type.const_int(6, false).into(),
            ],
            "",
        )
        .unwrap();
    let dest = unsafe { builder.build_gep(i8_type, buf, &[i64_type.const_int(6, false)], "dest") }
        .unwrap();
    builder
        .build_call(memcpy, &[dest.into(), name.into(), name_len.into()], "")
        .unwrap();
    let nl_at = builder
        .build_int_add(name_len, i64_type.const_int(6, false), "nl_at")
        .unwrap();
    let nl_slot = unsafe { builder.build_gep(i8_type, buf, &[nl_at], "nl_slot") }.unwrap();
    builder
        .build_store(nl_slot, i8_type.const_int(b'\n' as u64, false))
        .unwrap();
    builder
        .build_call(write, &[one.into(), buf.into(), total.into()], "")
        .unwrap();
    builder.build_return(Some(&zero)).unwrap();

    module.verify().expect("module verifies");
    print!("{}", module.print_to_string().to_string());
    machine
        .write_to_file(&module, FileType::Object, Path::new(&out))
        .expect("write object");
    println!("; LLVM {} wrote {out}", llvm_version());
}

fn llvm_version() -> String {
    let (mut major, mut minor, mut patch) = (0, 0, 0);
    // SAFETY: LLVMGetVersion only writes three u32s.
    unsafe { inkwell::llvm_sys::core::LLVMGetVersion(&mut major, &mut minor, &mut patch) };
    format!("{major}.{minor}.{patch}")
}
