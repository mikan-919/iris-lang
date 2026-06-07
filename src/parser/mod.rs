pub use self::iris::ProgramParser;

#[allow(clippy::all)]
mod iris {
    include!(concat!(env!("OUT_DIR"), "/parser/iris.rs"));
}
