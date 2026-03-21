//! Direct C Simplicity evaluator with corrected FFI signatures.
//!
//! `simplicity-sys` ≤0.6.2 has a bug in its `evalTCOExpression` Rust binding:
//! the `minCost: ubounded` parameter is missing, shifting all subsequent args.
//! This module declares the correct signatures and provides a minimal
//! `run_program_with_env` that matches what `elementsd` does.
//!
//! Remove this module once `simplicity-sys` is updated with the fix (PR #355).

#![cfg(feature = "testing")]

use std::os::raw::{c_int, c_uchar, c_uint};
use std::ptr;

use simplicity_sys::CElementsTxEnv;

// --- Raw C types matching simplicity-sys internals ---

type Ubounded = u32;
type CSize = usize;
const UBOUNDED_MAX: Ubounded = u32::MAX;

#[repr(C)]
struct CBitstream {
    arr: *const c_uchar,
    len: CSize,
    offset: CSize,
}

impl CBitstream {
    fn from_slice(s: &[u8]) -> Self {
        CBitstream {
            arr: s.as_ptr(),
            len: s.len(),
            offset: 0,
        }
    }
}

// Opaque types — we only pass pointers to them.
#[repr(C)]
struct CDagNode {
    _opaque: [u8; 0],
}

#[repr(C)]
struct CType {
    _opaque: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
struct CCombinatorCounters {
    comp_cnt: usize,
    case_cnt: usize,
    pair_cnt: usize,
    disconnect_cnt: usize,
    injl_cnt: usize,
    injr_cnt: usize,
    take_cnt: usize,
    drop_cnt: usize,
}

type DecodeJetFn = unsafe extern "C" fn(*mut CDagNode, CSize, *mut CBitstream) -> c_int;

unsafe extern "C" {
    #[link_name = "rustsimplicity_0_6_decodeMallocDag"]
    fn decodeMallocDag(
        dag: *mut *mut CDagNode,
        decode_jet: DecodeJetFn,
        census: *mut CCombinatorCounters,
        stream: *mut CBitstream,
    ) -> c_int;

    #[link_name = "rustsimplicity_0_6_closeBitstream"]
    fn closeBitstream(stream: *mut CBitstream) -> c_int;

    #[link_name = "rustsimplicity_0_6_elements_decodeJet"]
    fn elements_decodeJet(dag: *mut CDagNode, i: CSize, stream: *mut CBitstream) -> c_int;

    #[link_name = "rustsimplicity_0_6_mallocTypeInference"]
    fn mallocTypeInference(
        type_dag: *mut *mut CType,
        malloc_bound_vars: unsafe extern "C" fn(
            *mut CType,
            CSize,
            *const CDagNode,
            CSize,
            *const CCombinatorCounters,
        ) -> c_int,
        dag: *const CDagNode,
        len: CSize,
        census: *const CCombinatorCounters,
    ) -> c_int;

    #[link_name = "rustsimplicity_0_6_elements_mallocBoundVars"]
    fn elements_mallocBoundVars(
        type_dag: *mut CType,
        offset: CSize,
        dag: *const CDagNode,
        len: CSize,
        census: *const CCombinatorCounters,
    ) -> c_int;

    #[link_name = "rustsimplicity_0_6_fillWitnessData"]
    fn fillWitnessData(
        dag: *mut CDagNode,
        type_dag: *mut CType,
        len: CSize,
        stream: *mut CBitstream,
    ) -> c_int;

    #[link_name = "rustsimplicity_0_6_analyseBounds"]
    fn analyseBounds(
        cell_bound: *mut Ubounded,
        uword_bound: *mut Ubounded,
        frame_bound: *mut Ubounded,
        cost_bound: *mut Ubounded,
        max_cells: Ubounded,
        min_cost: Ubounded,
        max_cost: Ubounded,
        dag: *const CDagNode,
        type_dag: *const CType,
        len: CSize,
    ) -> c_int;

    /// Corrected binding — includes `min_cost` that `simplicity-sys` omits.
    #[link_name = "rustsimplicity_0_6_evalTCOExpression"]
    fn evalTCOExpression(
        anti_dos_checks: c_uchar,
        output: *mut c_uint,
        input: *const c_uint,
        dag: *const CDagNode,
        type_dag: *mut CType,
        len: CSize,
        min_cost: Ubounded,
        budget: *const Ubounded,
        env: *const CElementsTxEnv,
    ) -> c_int;
}

fn sim_free(ptr: *mut u8) {
    unsafe { simplicity_sys::alloc::rust_0_6_free(ptr) }
}

fn check(code: c_int, label: &str) -> Result<usize, String> {
    if code < 0 {
        Err(format!("C {label} failed with code {code}"))
    } else {
        Ok(code as usize)
    }
}

/// Run a Simplicity program through the full C pipeline with a real env.
pub fn run_program_with_env(
    program: &[u8],
    witness: &[u8],
    env: &CElementsTxEnv,
) -> Result<(), String> {
    unsafe {
        let mut prog_stream = CBitstream::from_slice(program);
        let mut wit_stream = CBitstream::from_slice(witness);
        let mut census = CCombinatorCounters::default();

        // 1. Decode
        let mut dag: *mut CDagNode = ptr::null_mut();
        let len = check(
            decodeMallocDag(&mut dag, elements_decodeJet, &mut census, &mut prog_stream),
            "decodeMallocDag",
        )?;
        assert!(!dag.is_null());
        struct DagGuard(*mut CDagNode);
        impl Drop for DagGuard {
            fn drop(&mut self) {
                sim_free(self.0 as *mut u8)
            }
        }
        let _g1 = DagGuard(dag);

        check(closeBitstream(&mut prog_stream), "closeBitstream(prog)")?;

        // 2. Type inference
        let mut type_dag: *mut CType = ptr::null_mut();
        check(
            mallocTypeInference(&mut type_dag, elements_mallocBoundVars, dag, len, &census),
            "mallocTypeInference",
        )?;
        assert!(!type_dag.is_null());
        struct TypeGuard(*mut CType);
        impl Drop for TypeGuard {
            fn drop(&mut self) {
                sim_free(self.0 as *mut u8)
            }
        }
        let _g2 = TypeGuard(type_dag);

        // 3. Fill witness
        check(
            fillWitnessData(dag, type_dag, len, &mut wit_stream),
            "fillWitnessData",
        )?;
        check(closeBitstream(&mut wit_stream), "closeBitstream(wit)")?;

        // 4. Analyse bounds
        let mut cell_bound: Ubounded = 0;
        let mut word_bound: Ubounded = 0;
        let mut frame_bound: Ubounded = 0;
        let mut cost_bound: Ubounded = 0;
        check(
            analyseBounds(
                &mut cell_bound,
                &mut word_bound,
                &mut frame_bound,
                &mut cost_bound,
                UBOUNDED_MAX,
                0,
                UBOUNDED_MAX,
                dag,
                type_dag,
                len,
            ),
            "analyseBounds",
        )?;

        // 5. Execute with corrected FFI
        let result = evalTCOExpression(
            0xFF, // CHECK_ALL
            ptr::null_mut(),
            ptr::null(),
            dag,
            type_dag,
            len,
            0,           // min_cost
            ptr::null(), // budget (NULL = use analysed bounds)
            env,
        );

        if result == 0 {
            Ok(())
        } else {
            Err(format!("C evalTCOExpression returned error code {result}"))
        }
    }
}
