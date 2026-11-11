//! Relations recorded by a single compaction, SPIR-V write, or SPIR-V parse.
//!
//! External tools can use these reports to carry their own source-operation
//! associations through Naga without guessing handles or SPIR-V IDs. Reports
//! describe the exact input and output of one call; subsequent transformations
//! invalidate them. Keep each report with its corresponding modules and words.
//!
//! Reporting does not change liveness, validation, capabilities, or lowering.
//! Validate modules normally before writing and after parsing. These relations
//! are not source provenance, numerical guarantees, or a serialization format.
//!
//! Compaction inventories every old owner and every handle in the listed arenas,
//! including removed objects. Emission inventories all function-local expressions
//! and records their terminal cached value IDs. Several expressions may share an
//! ID; intermediate lowering instructions are not inventoried.
//!
//! Direct emission correspondence covers scalar/equal-vector Add, Subtract,
//! Multiply and floating-point Dot. Other operations and expanded lowerings have
//! only value correspondence, if available. Import records arithmetic parse
//! events, resolves owners after wrapping and reordering, and reports Unsupported
//! if adaptation or postprocessing prevents a direct relation. An operand edge
//! does not establish memory, phi, or reaching-definition provenance.
use crate::{Expression, Function, Handle};
use alloc::vec::Vec;

/// Revision of the report contract, not an identifier for a module.
pub const REVISION: u32 = 1;
/// Arena that owns a function-local expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Function(Handle<Function>),
    EntryPoint(usize),
}
/// A function-local handle qualified by its owner in one exact module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExpressionRef {
    pub owner: Owner,
    pub expression: Handle<Expression>,
}
/// The fate of an old handle in the reported compaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition<T> {
    Survives(T),
    Removed,
}
/// One old arena handle and its disposition in the compacted module.
#[derive(Clone, Debug)]
pub struct HandleRemap<T> {
    pub old: Handle<T>,
    pub new: Disposition<Handle<T>>,
}
/// Owner and expression dispositions, including every removed expression.
#[derive(Clone, Debug)]
pub struct OwnerRemap {
    pub old: Owner,
    pub new: Disposition<Owner>,
    pub expressions: Vec<(Handle<Expression>, Disposition<ExpressionRef>)>,
}
/// Actual maps used by compaction. Local variables and argument ordinals
/// remain unchanged within surviving owners.
#[derive(Clone, Debug)]
pub struct CompactionReport {
    pub revision: u32,
    pub owners: Vec<OwnerRemap>,
    pub types: Vec<HandleRemap<crate::Type>>,
    pub globals: Vec<HandleRemap<crate::GlobalVariable>>,
    pub constants: Vec<HandleRemap<crate::Constant>>,
    pub overrides: Vec<HandleRemap<crate::Override>>,
    pub global_expressions: Vec<HandleRemap<Expression>>,
}
/// Exact direct instruction observed at emission, not reconstructed from words.
#[derive(Clone, Debug)]
pub struct EmittedArithmetic {
    pub producer: ExpressionRef,
    pub opcode: u32,
    pub type_id: u32,
    pub result_id: u32,
    pub arguments: [(ExpressionRef, u32); 2],
}
/// Terminal value disposition for one input expression.
#[derive(Clone, Debug)]
pub enum EmittedValue {
    /// The owner was excluded by the writer's actual selection policy.
    OwnerNotEmitted,
    /// Owner emitted, but no cached value ID (dead or pointer-only expression).
    NoValueId,
    /// Terminal cached value only. May alias other expressions, or be the final
    /// result of multiple lowering instructions. Not a one-to-one instruction map.
    Value(u32),
}
/// Relations from the input module to the words returned by the same write.
#[derive(Clone, Debug)]
pub struct EmissionReport {
    pub revision: u32,
    pub values: Vec<(ExpressionRef, EmittedValue)>,
    /// Direct scalar/equal-vector Add/Sub/Mul and float Dot instructions.
    pub arithmetic: Vec<EmittedArithmetic>,
}
/// An original SPIR-V arithmetic result and its final parsed disposition.
#[derive(Clone, Debug)]
pub enum ImportedArithmetic {
    Direct {
        function_id: u32,
        result_id: u32,
        type_id: u32,
        producer: ExpressionRef,
        arguments: [(u32, ExpressionRef); 2],
    },
    /// No successful mapping is asserted for sign adaptation, spill/reload or
    /// an unexpected postprocessing change. The result ID remains inventoried.
    Unsupported { function_id: u32, result_id: u32 },
}
/// Arithmetic parse events resolved into the final returned module.
/// Entry bodies belong to module functions called by synthetic entry wrappers.
#[derive(Clone, Debug)]
pub struct ImportReport {
    pub revision: u32,
    /// Add/Sub/Mul and Dot parse events, including multiplication instructions
    /// with vector/matrix operands. Other operations have no inventory here.
    pub arithmetic: Vec<ImportedArithmetic>,
}
/// Classification for one requested producer. `ValueOnly` explicitly refuses
/// instruction-level correspondence (aliases and expanded lowering included).
#[derive(Clone, Copy, Debug)]
pub enum ArithmeticEmission<'a> {
    Direct(&'a EmittedArithmetic),
    ValueOnly(u32),
    OwnerNotEmitted,
    NoValueId,
    UnknownSource,
}
impl EmissionReport {
    /// Look up a source expression. This performs a linear scan of the report.
    pub fn producer(&self, source: ExpressionRef) -> ArithmeticEmission<'_> {
        if let Some(a) = self.arithmetic.iter().find(|a| a.producer == source) {
            return ArithmeticEmission::Direct(a);
        }
        match self
            .values
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, v)| v)
        {
            Some(EmittedValue::Value(id)) => ArithmeticEmission::ValueOnly(*id),
            Some(EmittedValue::OwnerNotEmitted) => ArithmeticEmission::OwnerNotEmitted,
            Some(EmittedValue::NoValueId) => ArithmeticEmission::NoValueId,
            None => ArithmeticEmission::UnknownSource,
        }
    }
}
