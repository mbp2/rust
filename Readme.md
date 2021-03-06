# Cranelift codegen backend for rust

The goal of this project is to create an alternative codegen backend for the rust compiler based on [Cranelift](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift).
This has the potential to improve compilation times in debug mode.
If your project doesn't use any of the things listed under "Not yet supported", it should work fine.
If not please open an issue.

## Building and testing

```bash
$ git clone https://github.com/bjorn3/rustc_codegen_cranelift.git
$ cd rustc_codegen_cranelift
$ ./prepare.sh # download and patch sysroot src and install hyperfine for benchmarking
$ ./build.sh
```

To run the test suite replace the last command with:

```bash
$ ./test.sh
```

This will implicitly build cg_clif too. Both `build.sh` and `test.sh` accept a `--debug` argument to
build in debug mode.

Alternatively you can download a pre built version from [GHA]. It is listed in the artifacts section
of workflow runs. Unfortunately due to GHA restrictions you need to be logged in to access it.

[GHA]: https://github.com/bjorn3/rustc_codegen_cranelift/actions?query=branch%3Amaster+event%3Apush+is%3Asuccess

## Usage

rustc_codegen_cranelift can be used as a near-drop-in replacement for `cargo build` or `cargo run` for existing projects.

Assuming `$cg_clif_dir` is the directory you cloned this repo into and you followed the instructions (`prepare.sh` and `build.sh` or `test.sh`).

In the directory with your project (where you can do the usual `cargo build`), run:

```bash
$ $cg_clif_dir/build/cargo.sh build
```

This will build your project with rustc_codegen_cranelift instead of the usual LLVM backend.

For additional ways to use rustc_codegen_cranelift like the JIT mode see [usage.md](docs/usage.md).

## Env vars

See [env_vars.md](docs/env_vars.md) for all env vars used by rustc_codegen_cranelift.

## Not yet supported

* Inline assembly ([no cranelift support](https://github.com/bytecodealliance/wasmtime/issues/1041))
    * On Linux there is support for invoking an external assembler for `global_asm!` and `asm!`.
      `llvm_asm!` will remain unimplemented forever. `asm!` doesn't yet support reg classes. You
      have to specify specific registers instead.
* SIMD ([tracked here](https://github.com/bjorn3/rustc_codegen_cranelift/issues/171), some basic things work)
