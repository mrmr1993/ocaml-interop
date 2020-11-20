// Copyright (c) SimpleStaking and Tezedge Contributors
// SPDX-License-Identifier: MIT

#![doc(html_root_url = "https://docs.rs/ocaml-interop/0.4.4")]

//! _Zinc-iron alloy coating is used in parts that need very good corrosion protection._
//!
//! [ocaml-interop](https://github.com/simplestaking/ocaml-interop) is an OCaml<->Rust FFI with an emphasis on safety inspired by [caml-oxide](https://github.com/stedolan/caml-oxide) and [ocaml-rs](https://github.com/zshipko/ocaml-rs).
//!
//! ## Table of Contents
//!
//! - [How does it work](#how-does-it-work)
//! - [Usage](#usage)
//!   * [Rules](#rules)
//!     + [Rule 1: OCaml function calls, allocations and the GC Frame](#rule-1-ocaml-function-calls-allocations-and-the-gc-frame)
//!     + [Rule 2: OCaml value references](#rule-2-ocaml-value-references)
//!     + [Rule 3: Liveness and scope of OCaml values](#rule-3-liveness-and-scope-of-ocaml-values)
//!   * [Converting between OCaml and Rust data](#converting-between-ocaml-and-rust-data)
//!     + [`FromOCaml` trait](#fromocaml-trait)
//!     + [`ToRust` trait](#torust-trait)
//!     + [`ToOCaml` trait](#toocaml-trait)
//!   * [Calling into OCaml from Rust](#calling-into-ocaml-from-rust)
//!   * [Calling into Rust from OCaml](#calling-into-rust-from-ocaml)
//! - [References and links](#references-and-links)
//!
//! ## How does it work
//!
//! ocaml-interop, just like [caml-oxide](https://github.com/stedolan/caml-oxide), encodes the invariants of OCaml's garbage collector into the rules of Rust's borrow checker. Any violation of these invariants results in a compilation error produced by Rust's borrow checker.
//!
//! This requires that the user is explicit about delimiting blocks that interact with the OCaml runtime, and that calls into the OCaml runtime are done only inside these blocks, and wrapped by a few special macros.
//!
//! ## Usage
//!
//! ### Rules
//!
//! There are a few rules that have to be followed when calling into the OCaml runtime:
//!
//! #### Rule 1: OCaml function calls, allocations and the GC Frame
//!
//! Calls into the OCaml runtime that perform allocations should only occur inside [`ocaml_frame!`] blocks, wrapped by either the [`ocaml_call!`] (for declared OCaml functions) or [`ocaml_alloc!`] (for allocation or conversion functions) macros.
//!
//! Example:
//!
//! ```rust,no_run
//! # use ocaml_interop::*;
//! # ocaml! { fn ocaml_function(arg1: String); }
//! # let a_string = "string";
//! # let arg1 = "arg1";
//! # let cr = unsafe { &mut OCamlRuntime::acquire() };
//! let arg1 = ocaml_alloc!(arg1.to_ocaml(cr));
//! // ...
//! let result = ocaml_call!(ocaml_function(cr, arg1, /* ...,  argN */));
//! let ocaml_string: OCaml<String> = ocaml_alloc!(a_string.to_ocaml(cr));
//! // ...
//! ```
//!
//! Without the macros, this error is produced, because without the macros an incorrect token is passed as the first argument:
//!
//! ```text,no_run
//! error[E0308]: mismatched types
//!   --> example.rs
//!    |
//!    |  let result = ocaml_function(cr, arg1, ..., argN);
//!    |                              ^^ expected struct `ocaml_interop::OCamlAllocToken`, found `&mut ocaml_interop::OCamlRuntime<'_>`
//! ```
//!
//! #### Rule 2: OCaml value references
//!
//! OCaml values that are obtained as a result of calling an OCaml function can only be referenced directly until another call to an OCaml function happens. This is enforced by Rust's borrow checker. If a value has to be referenced after other OCaml function calls, a special reference has to be kept.
//!
//! Example:
//!
//! ```rust,no_run
//! # use ocaml_interop::*;
//! # ocaml! {
//! #     fn ocaml_function(arg1: String) -> String;
//! #     fn another_ocaml_function(arg: String);
//! # }
//! # let a_string = "string";
//! # let arg1 = "arg1";
//! # let arg2 = "arg2";
//! # let cr = unsafe { &mut OCamlRuntime::acquire() };
//! ocaml_frame!(cr(result_ref), {
//!     let arg1 = ocaml_alloc!(arg1.to_ocaml(cr));
//!     let result = ocaml_call!(ocaml_function(cr, arg1, /* ..., argN */)).unwrap();
//!     let result_ref = &result_ref.keep(result);
//!     let arg2 = ocaml_alloc!(arg2.to_ocaml(cr));
//!     let another_result = ocaml_call!(ocaml_function(cr, arg2, /* ..., argN */)).unwrap();
//!     // ...
//!     let more_results = ocaml_call!(another_ocaml_function(cr, cr.get(result_ref))).unwrap();
//!     // ...
//! })
//! ```
//!
//! If the value is not kept with `root_var.keep` (root variables are declared when opening an [`ocaml_frame!`]), and instead an attempt is made to re-use it directly, Rust's borrow checker will complain:
//!
//! ```text,no_run
//! error[E0502]: cannot borrow `*cr` as mutable because it is also borrowed as immutable
//!   --> example.rs
//!    |
//!    |  let result = ocaml_call!(ocaml_function(cr, arg1, ..., argN)).unwrap();
//!    |               ------------------------------------ immutable borrow occurs here
//! ...
//!    |  let another_result = ocaml_call!(ocaml_function(cr, arg1, ..., argN)).unwrap();
//!    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
//! ...
//!    |  let more_results = ocaml_call!(another_ocaml_function(cr, result)).unwrap();
//!    |                                                            ------ immutable borrow later used here
//!    |
//! ```
//!
//! There is no need to keep values that are used immediately without any calls into the OCaml runtime in-between their allocation and use.
//!
//! #### Rule 3: Liveness and scope of OCaml values
//!
//! TODO: update this whole section, it is not correct anymore
//!
//! OCaml values that are the result of an allocation by the OCaml runtime cannot escape the [`ocaml_frame!`] block inside which they where created. This is enforced by Rust's borrow checker.
//!
//! Example:
//!
//! ```rust,no_run
//! # use ocaml_interop::*;
//! # ocaml! {
//! #     fn ocaml_function(arg1: String) -> String;
//! #     fn another_ocaml_function(arg: String);
//! # }
//! # let a_string = "string";
//! # let arg1 = "arg1";
//! # let cr = unsafe { &mut OCamlRuntime::acquire() };
//! let arg1 = ocaml_alloc!(arg1.to_ocaml(cr));
//! let result = ocaml_call!(ocaml_function(cr, arg1, /* ..., argN */)).unwrap();
//! let s = String::from_ocaml(result);
//! // ...
//! ```
//!
//! If the result escapes the block, Rust's borrow checker will complain:
//!
//! ```text,no_run
//! error[E0597]: `frame` does not live long enough
//!   --> example.rs
//!    |
//!    |       let s = ocaml_frame!(gc, {
//!    |  _________-___^
//!    | |         |
//!    | |         borrow later stored here
//!    | |         let result = let result = ocaml_call!(ocaml_function(gc, arg1, ..., argN)).unwrap();
//!    | |         result
//!    | |     });
//!    | |      ^
//!    | |      |
//!    | |______borrowed value does not live long enough
//!    |        `frame` dropped here while still borrowed
//!    |
//! ```
//!
//! **TODO**: show escape hatch for values that need to escape the frame scope using raw OCaml values.
//!
//! ### Converting between OCaml and Rust data
//!
//! #### [`FromOCaml`] trait
//!
//! The [`FromOCaml`] trait implements conversion from OCaml values into Rust values, using the `from_ocaml` function.
//!
//! #### [`ToRust`] trait
//!
//! [`ToRust`] is the counterpart to [`FromOCaml`] similar to how `Into` is to `From`. Using `ocaml_val.to_rust()` instead of `Type::from_ocaml(ocaml_val)` is usually more convenient, specially when more complicated types are involved.
//!
//! #### [`ToOCaml`] trait
//!
//! The [`ToOCaml`] trait implements conversion from Rust values into OCaml values, using the `to_ocaml` function. `to_ocaml` can only be called when wrapped by the [`ocaml_alloc!`] macro form, and it takes a single parameter that must be a handle to the current GC frame.
//!
//! ### Calling into OCaml from Rust
//!
//! The following code defines two OCaml functions and registers them using the `Callback.register` mechanism:
//!
//! ```ocaml
//! let increment_bytes bytes first_n =
//!   let limit = (min (Bytes.length bytes) first_n) - 1 in
//!   for i = 0 to limit do
//!     let value = (Bytes.get_uint8 bytes i) + 1 in
//!     Bytes.set_uint8 bytes i value
//!   done;
//!   bytes
//!
//! let twice x = 2 * x
//!
//! let () =
//!   Callback.register "increment_bytes" increment_bytes;
//!   Callback.register "twice" twice
//! ```
//!
//! To be able to call these from Rust, there are a few things that need to be done:
//!
//! - The OCaml runtime has to be initialized. If the driving program is a Rust application, it has to be done explicitly by doing `let runtime = OCamlRuntime::init()`, but if the driving program is an OCaml application, this is not required. When `runtime` goes out of scope it will be dropped and the OCaml runtime cleanup functions will be executed.
//! - Functions that were exported from the OCaml side with `Callback.register` have to be declared using the [`ocaml!`] macro.
//! - Blocks of code that call OCaml functions, or allocate OCaml values, must be wrapped by the [`ocaml_frame!`] macro.
//! - Calls to functions that allocate OCaml values must be wrapped by the [`ocaml_alloc!`] macro. These always return a value and cannot signal failure.
//! - Calls to functions exported by OCaml with `Callback.register` must be wrapped by the [`ocaml_call!`] macro. These return a value of type `Result<OCaml<T>, ocaml_interop::Error>`, with the error being returned to signal that an exception was raised by the called OCaml code.
//!
//! #### Example
//!
//! ```rust,no_run
//! use ocaml_interop::{
//!     ocaml_alloc, ocaml_call, ocaml_frame, to_ocaml, ToRust, FromOCaml, OCaml, OCamlRef, ToOCaml,
//!     OCamlRuntime
//! };
//!
//! // To call an OCaml function, it first has to be declared inside an `ocaml!` macro block:
//! mod ocaml_funcs {
//!     use ocaml_interop::{ocaml, OCamlInt};
//!
//!     ocaml! {
//!         // OCaml: `val increment_bytes: bytes -> int -> bytes`
//!         // registered with `Callback.register "increment_bytes" increment_bytes`
//!         pub fn increment_bytes(bytes: String, first_n: OCamlInt) -> String;
//!         // OCaml: `val twice: int -> int`
//!         // registered with `Callback.register "twice" twice`
//!         pub fn twice(num: OCamlInt) -> OCamlInt;
//!     }
//!
//!     // The two OCaml functions declared above can now be invoked with the
//!     // `ocaml_call!` macro: `ocaml_call!(func_name(cr, args...))`.
//!     // Note the first `cr` parameter, it is an OCaml Runtime handle.
//! }
//!
//! fn increment_bytes(cr: &mut OCamlRuntime, bytes1: String, bytes2: String, first_n: usize) -> (String, String) {
//!     // Any calls into the OCaml runtime have to happen inside an
//!     // `ocaml_frame!` block. Inside this block, OCaml allocations and references
//!     // to OCaml allocated values are tracked and validated by Rust's borrow checker.
//!     // The first argument to the macro is a name for the GC handle, followed by an optional
//!     // list of "root variables" (more on this later). The second argument
//!     // is the block of code that will run inside that frame.
//!     ocaml_frame!(cr(bytes1_ref, bytes2_ref), {
//!         // The `ToOCaml` trait provides the `to_ocaml` method to convert Rust
//!         // values into OCaml values. Because such conversions usually require
//!         // the OCaml runtime to perform an allocation, calls to `to_ocaml` have
//!         // to be wrapped by the `ocaml_alloc!` macro. A shorter version uses
//!         // the `to_ocaml!` macro.
//!         let ocaml_bytes1: OCaml<String> = to_ocaml!(cr, bytes1);
//!
//!         // `ocaml_bytes1` is going to be referenced later, but there calls into the
//!         // OCaml runtime that perform allocations happening before this value is used again.
//!         // Those calls into the OCaml runtime invalidate this reference, so it has to be
//!         // kept alive somehow. To do so, `bytes_ref.keep(ocaml_bytes1)` is used.
//!         // `bytes_ref` is one of the "root variables" that were declared when opening this frame.
//!         // Each "root variable" reserves space for a reference that will be tracked by the GC.
//!         // A root variable's `root_var.keep(value)` method returns
//!         // a reference to an OCaml value that is going to be valid during the scope of
//!         // the current `ocaml_frame!` block. Later `cr.get(the_reference)` can be used
//!         // to obtain the kept value.
//!         let bytes1_ref: &OCamlRef<String> = &bytes1_ref.keep(ocaml_bytes1);
//!
//!         // Same as above. Note that if we waited to perform this conversion
//!         // until after `ocaml_bytes1` is used, no references would have to be
//!         // kept for either of the two OCaml values, because they would be
//!         // used immediately, with no allocations being performed by the
//!         // OCaml runtime in-between.
//!         // Here a third argument is passed to `to_ocaml!`, a root variable.
//!         // This variation returns an `OCamlRef` value instead of an `OCaml` one.
//!         let bytes2_ref = &to_ocaml!(cr, bytes2, bytes2_ref);
//!
//!         // Rust `i64` integers can be converted into OCaml fixnums with `OCaml::of_i64` and `OCaml::of_i64_unchecked`.
//!         // Such conversion doesn't require any allocation on the OCaml side,
//!         // so this call doesn't have to be wrapped by `ocaml_alloc!` or `to_ocaml!`,
//!         // and no GC handle is passed as an argument.
//!         let ocaml_first_n = unsafe { OCaml::of_i64_unchecked(first_n as i64) };
//!
//!         // To call an OCaml function (declared above in a `ocaml!` block) the
//!         // `ocaml_call!` macro is used. The GC handle has to be passed as the first argument,
//!         // before all the other declared arguments.
//!         // The result of this call is a Result<OCamlValue<T>, ocaml_interop::Error>, with `Err(...)`
//!         // being the result of calls for which the OCaml runtime raises an exception.
//!         let result1 = ocaml_call!(ocaml_funcs::increment_bytes(
//!             cr,
//!             // The reference created above is used here to obtain the value
//!             // of `ocaml_bytes1`
//!             cr.get(bytes1_ref),
//!             ocaml_first_n
//!         )).unwrap();
//!
//!         // Perform the conversion of the OCaml result value into a
//!         // Rust value while the reference is still valid because the
//!         // `ocaml_call!` that follows will invalidate it.
//!         // Alternatively, the result of `rootvar.keep(result1)` could be used
//!         // to be able to reference the value later through an `OCamlRef` value.
//!         let new_bytes1: String = result1.to_rust();
//!         let result2 = ocaml_call!(ocaml_funcs::increment_bytes(
//!             cr,
//!             cr.get(bytes2_ref),
//!             ocaml_first_n
//!         )).unwrap();
//!
//!         // The `FromOCaml` trait provides the `from_ocaml` method to convert from
//!         // OCaml values into OCaml values. Unlike the `to_ocaml` method, it doesn't
//!         // require a GC handle argument, because no allocation is performed by the
//!         // OCaml runtime when converting into Rust values.
//!         // A more convenient alternative, is to use the `to_rust` method as
//!         // above when `result1` was converted.
//!         (new_bytes1, String::from_ocaml(result2))
//!     })
//! }
//!
//! fn twice(cr: &mut OCamlRuntime, num: usize) -> usize {
//!     let ocaml_num = unsafe { OCaml::of_i64_unchecked(num as i64) };
//!     let result = ocaml_call!(ocaml_funcs::twice(cr, ocaml_num));
//!     i64::from_ocaml(result.unwrap()) as usize
//! }
//!
//! fn entry_point() {
//!     // IMPORTANT: the OCaml runtime has to be initialized first.
//!     let mut ocaml_runtime = OCamlRuntime::init();
//!     let first_n = twice(&mut ocaml_runtime, 5);
//!     let bytes1 = "000000000000000".to_owned();
//!     let bytes2 = "aaaaaaaaaaaaaaa".to_owned();
//!     println!("Bytes1 before: {}", bytes1);
//!     println!("Bytes2 before: {}", bytes2);
//!     let (result1, result2) = increment_bytes(&mut ocaml_runtime, bytes1, bytes2, first_n);
//!     println!("Bytes1 after: {}", result1);
//!     println!("Bytes2 after: {}", result2);
//!     // At this point the `ocaml_runtime` handle will be dropped, triggering
//!     // the execution of the necessary cleanup by the OCaml runtime
//! }
//! ```
//!
//! ### Calling into Rust from OCaml
//!
//! To be able to call a Rust function from OCaml, it has to be defined in a way that exposes it to OCaml. This can be done with the [`ocaml_export!`] macro.
//!
//! #### Example
//!
//! ```rust,no_run
//! use ocaml_interop::{to_ocaml, ocaml_export, ocaml_frame, FromOCaml, OCamlInt, OCaml, OCamlBytes, ToOCaml};
//!
//! // `ocaml_export` expands the function definitions by adding `pub` visibility and
//! // the required `#[no_mangle]` and `extern` declarations. It also takes care of
//! // binding the GC frame handle to the name provided as the first parameter (along with
//! // an optional list of "root variables", like in `ocaml_frame!`).
//! ocaml_export! {
//!     // The first parameter is a name to which the GC frame handle will be bound to.
//!     // The remaining parameters and return value must have a declared type of `OCaml<T>`.
//!     fn rust_twice(_cr, num: OCaml<OCamlInt>) -> OCaml<OCamlInt> {
//!         let num = i64::from_ocaml(num);
//!         unsafe { OCaml::of_i64_unchecked(num * 2) }
//!     }
//!
//!     fn rust_increment_bytes(cr, bytes: OCaml<OCamlBytes>, first_n: OCaml<OCamlInt>) -> OCaml<OCamlBytes> {
//!         let first_n = i64::from_ocaml(first_n) as usize;
//!         let mut vec = Vec::from_ocaml(bytes);
//!
//!         for i in 0..first_n {
//!             vec[i] += 1;
//!         }
//!
//!         // Note that unlike in `ocaml_frame!` blocks, where values of type `OCaml<T>`
//!         // cannot escape, in functions defined inside `ocaml_export!` blocks,
//!         // only results of type `OCaml<T>` are valid.
//!         to_ocaml!(cr, vec)
//!     }
//! }
//! ```
//!
//! Then in OCaml, these functions can be referred to in the same way as C functions:
//!
//! ```ocaml
//! external rust_twice: int -> int = "rust_twice"
//! external rust_increment_bytes: bytes -> int -> bytes = "rust_increment_bytes"
//! ```
//!
//! ## References and links
//!
//! - OCaml Manual: [Chapter 20  Interfacing C with OCaml](https://caml.inria.fr/pub/docs/manual-ocaml/intfc.html).
//! - [Safely Mixing OCaml and Rust](https://docs.google.com/viewer?a=v&pid=sites&srcid=ZGVmYXVsdGRvbWFpbnxtbHdvcmtzaG9wcGV8Z3g6NDNmNDlmNTcxMDk1YTRmNg) paper by Stephen Dolan.
//! - [Safely Mixing OCaml and Rust](https://www.youtube.com/watch?v=UXfcENNM_ts) talk by Stephen Dolan.
//! - [CAMLroot: revisiting the OCaml FFI](https://arxiv.org/abs/1812.04905).
//! - [caml-oxide](https://github.com/stedolan/caml-oxide), the code from that paper.
//! - [ocaml-rs](https://github.com/zshipko/ocaml-rs), another OCaml<->Rust FFI library.

mod closure;
mod conv;
mod error;
mod macros;
mod memory;
mod mlvalues;
mod runtime;
mod value;

pub use crate::closure::{OCamlFn1, OCamlFn2, OCamlFn3, OCamlFn4, OCamlFn5, OCamlResult};
pub use crate::conv::{FromOCaml, ToOCaml, ToRust};
pub use crate::error::{OCamlError, OCamlException};
pub use crate::memory::{OCamlAllocResult, OCamlRef};
pub use crate::mlvalues::{
    OCamlBytes, OCamlFloat, OCamlInt, OCamlInt32, OCamlInt64, OCamlList, RawOCaml,
};
pub use crate::runtime::{OCamlAllocToken, OCamlRuntime};
pub use crate::value::OCaml;

#[doc(hidden)]
pub mod internal {
    pub use crate::closure::OCamlClosure;
    pub use crate::memory::{caml_alloc, store_field, OCamlRoot};
    pub use crate::mlvalues::UNIT;
    pub use ocaml_sys::int_val;
}

#[doc(hidden)]
#[cfg(doctest)]
pub mod compile_fail_tests;
