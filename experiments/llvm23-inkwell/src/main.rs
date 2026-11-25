//! PROTOTYPE (ci-ajns): emit a native object whose `main` prints `hello world\n`.
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
    let ptr_type = context.ptr_type(AddressSpace::default());

    let puts = module.add_function("puts", i32_type.fn_type(&[ptr_type.into()], false), None);
    let main = module.add_function("main", i32_type.fn_type(&[], false), None);
    builder.position_at_end(context.append_basic_block(main, "entry"));
    let text = builder
        .build_global_string_ptr("hello world", "text")
        .unwrap();
    builder
        .build_call(puts, &[text.as_pointer_value().into()], "")
        .unwrap();
    builder.build_return(Some(&i32_type.const_zero())).unwrap();

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
