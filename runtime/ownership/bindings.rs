// Shared by the archive build (checks the Rust items exist) and compiler
// (pins each operation name). These are Rust items, not Rill ABI symbols.
macro_rules! ownership_bindings {
    ($define:ident) => {
        $define! {
            SharedRetain => shared::retain, "retainShared", "{ return retainShared(value); }";
            SharedRelease => shared::release, "releaseShared", "{ releaseShared(value); }";
            GuardRelease => mutex::release, "releaseGuard", "{ releaseGuard(value); }";
            SenderRetain => channel::retain_sender, "retainSender", "{ return retainSender(value); }";
            SenderRelease => channel::release_sender, "releaseSender", "{ releaseSender(value); }";
            ReceiverRelease => channel::release_receiver, "releaseReceiver", "{ releaseReceiver(value); }";
        }
    };
}
