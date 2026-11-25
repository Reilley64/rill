use std::path::Path;
use std::process::Command;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

fn write_object_returning_42(object: &Path) {
    Target::initialize_native(&InitializationConfig::default()).expect("native target");
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).expect("target for the host triple");
    let machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .expect("target machine");

    let context = Context::create();
    let module = context.create_module("smoke");
    module.set_triple(&triple);
    module.set_data_layout(&machine.get_target_data().get_data_layout());

    let i32_type = context.i32_type();
    let main = module.add_function("main", i32_type.fn_type(&[], false), None);
    let builder = context.create_builder();
    builder.position_at_end(context.append_basic_block(main, "entry"));
    builder
        .build_return(Some(&i32_type.const_int(42, false)))
        .expect("return");

    module
        .verify()
        .expect("the LLVM verifier accepts the module");
    machine
        .write_to_file(&module, FileType::Object, object)
        .expect("object file");
}

#[test]
fn a_module_returning_42_builds_links_and_runs() {
    let directory =
        std::env::temp_dir().join(format!("rill-codegen-llvm-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    let object = directory.join("smoke.o");
    let executable = directory.join("smoke");

    write_object_returning_42(&object);
    let link = Command::new("cc")
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("host cc");
    assert!(link.success(), "host cc failed to link the object");
    let status = Command::new(&executable)
        .status()
        .expect("run the executable");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(status.code(), Some(42));
}
