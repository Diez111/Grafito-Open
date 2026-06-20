// Grafito 1D function compute evaluator.
//
// Interprets a small RPN bytecode stream to evaluate y = f(x) for every
// sample in parallel. The output is a 1D buffer of y values that the caller
// reads back and stores in the function object's cache.
//
// Bindings:
//   0 - uniform FunctionParams
//   1 - storage read bytecode (u32[])
//   2 - storage read constants (f32[])
//   3 - storage read_write values (f32[])

struct FunctionParams {
    x_min: f32,
    x_max: f32,
    n: u32,
    code_len: u32,
};

@group(0) @binding(0)
var<uniform> params: FunctionParams;

@group(0) @binding(1)
var<storage, read> bytecode: array<u32>;

@group(0) @binding(2)
var<storage, read> constants: array<f32>;

@group(0) @binding(3)
var<storage, read_write> values: array<f32>;

const OP_NOP: u32 = 0u;
const OP_PUSH_CONST: u32 = 1u;
const OP_PUSH_VAR: u32 = 2u;
const OP_ADD: u32 = 3u;
const OP_SUB: u32 = 4u;
const OP_MUL: u32 = 5u;
const OP_DIV: u32 = 6u;
const OP_POW: u32 = 7u;
const OP_NEG: u32 = 8u;
const OP_SIN: u32 = 9u;
const OP_COS: u32 = 10u;
const OP_TAN: u32 = 11u;
const OP_EXP: u32 = 12u;
const OP_LOG: u32 = 13u; // natural log
const OP_SQRT: u32 = 14u;
const OP_ABS: u32 = 15u;
const OP_MIN: u32 = 16u;
const OP_MAX: u32 = 17u;
const OP_FLOOR: u32 = 18u;
const OP_CEIL: u32 = 19u;
const OP_PI: u32 = 20u;
const OP_E: u32 = 21u;
// Extended opcodes
const OP_ASIN: u32 = 22u;
const OP_ACOS: u32 = 23u;
const OP_ATAN: u32 = 24u;
const OP_SINH: u32 = 25u;
const OP_COSH: u32 = 26u;
const OP_TANH: u32 = 27u;
const OP_ASINH: u32 = 28u;
const OP_ACOSH: u32 = 29u;
const OP_ATANH: u32 = 30u;
const OP_SEC: u32 = 31u;
const OP_CSC: u32 = 32u;
const OP_COT: u32 = 33u;
const OP_SIGN: u32 = 34u;
const OP_HEAVISIDE: u32 = 35u;
const OP_CBRT: u32 = 36u;
const OP_MOD: u32 = 37u;
const OP_ROUND: u32 = 38u;
const OP_LOG10: u32 = 39u;
const OP_LOG2: u32 = 40u;
const OP_EXP2: u32 = 41u;
const OP_ATAN2: u32 = 42u;
const OP_CLAMP: u32 = 43u;
const OP_LT: u32 = 44u;
const OP_GT: u32 = 45u;
const OP_LE: u32 = 46u;
const OP_GE: u32 = 47u;
const OP_EQ: u32 = 48u;
const OP_NE: u32 = 49u;

const STACK_SIZE: i32 = 32;

fn eval_bytecode(x: f32) -> f32 {
    var stack: array<f32, STACK_SIZE>;
    var sp: i32 = 0;

    let len = params.code_len;
    for (var pc: u32 = 0u; pc < len; pc = pc + 1u) {
        if sp < 0 || sp >= STACK_SIZE {
            var zero = 0.0;
            return zero / zero;
        }
        let instr = bytecode[pc];
        let op = instr & 0xFFu;
        let operand = instr >> 8u;

        switch op {
            case OP_PUSH_CONST: {
                stack[sp] = constants[operand];
                sp = sp + 1;
            }
            case OP_PUSH_VAR: {
                // Function objects are 1D: only x is available.
                stack[sp] = x;
                sp = sp + 1;
            }
            case OP_ADD: {
                sp = sp - 2;
                stack[sp] = stack[sp] + stack[sp + 1];
                sp = sp + 1;
            }
            case OP_SUB: {
                sp = sp - 2;
                stack[sp] = stack[sp] - stack[sp + 1];
                sp = sp + 1;
            }
            case OP_MUL: {
                sp = sp - 2;
                stack[sp] = stack[sp] * stack[sp + 1];
                sp = sp + 1;
            }
            case OP_DIV: {
                sp = sp - 2;
                let denom = stack[sp + 1];
                if abs(denom) > 1e-10 {
                    stack[sp] = stack[sp] / denom;
                } else {
                    var zero = 0.0;
                    stack[sp] = zero / zero;
                }
                sp = sp + 1;
            }
            case OP_POW: {
                sp = sp - 2;
                let b = stack[sp];
                let e = stack[sp + 1];
                if b < 0.0 {
                    if floor(e) == e {
                        let is_even = abs(e % 2.0) < 0.001;
                        if is_even {
                            stack[sp] = pow(-b, e);
                        } else {
                            stack[sp] = -pow(-b, e);
                        }
                    } else {
                        var zero = 0.0;
                        stack[sp] = zero / zero;
                    }
                } else {
                    stack[sp] = pow(b, e);
                }
                sp = sp + 1;
            }
            case OP_NEG: {
                sp = sp - 1;
                stack[sp] = -stack[sp];
                sp = sp + 1;
            }
            case OP_SIN: {
                sp = sp - 1;
                stack[sp] = sin(stack[sp]);
                sp = sp + 1;
            }
            case OP_COS: {
                sp = sp - 1;
                stack[sp] = cos(stack[sp]);
                sp = sp + 1;
            }
            case OP_TAN: {
                sp = sp - 1;
                stack[sp] = tan(stack[sp]);
                sp = sp + 1;
            }
            case OP_EXP: {
                sp = sp - 1;
                stack[sp] = exp(stack[sp]);
                sp = sp + 1;
            }
            case OP_LOG: {
                sp = sp - 1;
                let v = stack[sp];
                if v <= 0.0 {
                    var zero = 0.0;
                    stack[sp] = zero / zero;
                } else {
                    stack[sp] = log(v);
                }
                sp = sp + 1;
            }
            case OP_SQRT: {
                sp = sp - 1;
                let v = stack[sp];
                if v < 0.0 {
                    var zero = 0.0;
                    stack[sp] = zero / zero;
                } else {
                    stack[sp] = sqrt(v);
                }
                sp = sp + 1;
            }
            case OP_ABS: {
                sp = sp - 1;
                stack[sp] = abs(stack[sp]);
                sp = sp + 1;
            }
            case OP_MIN: {
                sp = sp - 2;
                stack[sp] = min(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_MAX: {
                sp = sp - 2;
                stack[sp] = max(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_FLOOR: {
                sp = sp - 1;
                stack[sp] = floor(stack[sp]);
                sp = sp + 1;
            }
            case OP_CEIL: {
                sp = sp - 1;
                stack[sp] = ceil(stack[sp]);
                sp = sp + 1;
            }
            case OP_PI: {
                stack[sp] = 3.14159265358979323846;
                sp = sp + 1;
            }
            case OP_E: {
                stack[sp] = 2.71828182845904523536;
                sp = sp + 1;
            }
            case OP_ASIN: {
                sp = sp - 1;
                let v = stack[sp];
                stack[sp] = select(asin(clamp(v, -1.0, 1.0)), asin(v), abs(v) <= 1.0);
                sp = sp + 1;
            }
            case OP_ACOS: {
                sp = sp - 1;
                let v = stack[sp];
                stack[sp] = select(acos(clamp(v, -1.0, 1.0)), acos(v), abs(v) <= 1.0);
                sp = sp + 1;
            }
            case OP_ATAN: {
                sp = sp - 1;
                stack[sp] = atan(stack[sp]);
                sp = sp + 1;
            }
            case OP_SINH: {
                sp = sp - 1;
                stack[sp] = sinh(stack[sp]);
                sp = sp + 1;
            }
            case OP_COSH: {
                sp = sp - 1;
                stack[sp] = cosh(stack[sp]);
                sp = sp + 1;
            }
            case OP_TANH: {
                sp = sp - 1;
                stack[sp] = tanh(stack[sp]);
                sp = sp + 1;
            }
            case OP_ASINH: {
                sp = sp - 1;
                stack[sp] = asinh(stack[sp]);
                sp = sp + 1;
            }
            case OP_ACOSH: {
                sp = sp - 1;
                let v = stack[sp];
                stack[sp] = select(acosh(max(v, 1.0)), acosh(v), v >= 1.0);
                sp = sp + 1;
            }
            case OP_ATANH: {
                sp = sp - 1;
                let v = stack[sp];
                stack[sp] = select(atanh(clamp(v, -0.99999994, 0.99999994)), atanh(v), abs(v) < 1.0);
                sp = sp + 1;
            }
            case OP_SEC: {
                sp = sp - 1;
                let v = stack[sp];
                let c = cos(v);
                if abs(c) < 1e-10 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = 1.0 / c; }
                sp = sp + 1;
            }
            case OP_CSC: {
                sp = sp - 1;
                let v = stack[sp];
                let s = sin(v);
                if abs(s) < 1e-10 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = 1.0 / s; }
                sp = sp + 1;
            }
            case OP_COT: {
                sp = sp - 1;
                let v = stack[sp];
                let t = tan(v);
                if abs(t) < 1e-10 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = 1.0 / t; }
                sp = sp + 1;
            }
            case OP_SIGN: {
                sp = sp - 1;
                let v = stack[sp];
                if v > 0.0 { stack[sp] = 1.0; } else if v < 0.0 { stack[sp] = -1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_HEAVISIDE: {
                sp = sp - 1;
                let v = stack[sp];
                if v >= 0.0 { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_CBRT: {
                sp = sp - 1;
                let v = stack[sp];
                if v < 0.0 { stack[sp] = -pow(-v, 1.0 / 3.0); } else { stack[sp] = pow(v, 1.0 / 3.0); }
                sp = sp + 1;
            }
            case OP_MOD: {
                sp = sp - 2;
                let b = stack[sp + 1];
                if abs(b) < 1e-10 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = stack[sp] - b * floor(stack[sp] / b); }
                sp = sp + 1;
            }
            case OP_ROUND: {
                sp = sp - 1;
                stack[sp] = floor(stack[sp] + 0.5);
                sp = sp + 1;
            }
            case OP_LOG10: {
                sp = sp - 1;
                let v = stack[sp];
                if v <= 0.0 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = log2(v) / 3.3219280948873626; }
                sp = sp + 1;
            }
            case OP_LOG2: {
                sp = sp - 1;
                let v = stack[sp];
                if v <= 0.0 { var z = 0.0; stack[sp] = z / z; } else { stack[sp] = log2(v); }
                sp = sp + 1;
            }
            case OP_EXP2: {
                sp = sp - 1;
                stack[sp] = exp2(stack[sp]);
                sp = sp + 1;
            }
            case OP_ATAN2: {
                sp = sp - 2;
                stack[sp] = atan2(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_CLAMP: {
                sp = sp - 3;
                stack[sp] = clamp(stack[sp], stack[sp + 1], stack[sp + 2]);
                sp = sp + 1;
            }
            case OP_LT: {
                sp = sp - 2;
                if stack[sp] < stack[sp + 1] { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_GT: {
                sp = sp - 2;
                if stack[sp] > stack[sp + 1] { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_LE: {
                sp = sp - 2;
                if stack[sp] <= stack[sp + 1] { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_GE: {
                sp = sp - 2;
                if stack[sp] >= stack[sp + 1] { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_EQ: {
                sp = sp - 2;
                if abs(stack[sp] - stack[sp + 1]) < 1e-10 { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            case OP_NE: {
                sp = sp - 2;
                if abs(stack[sp] - stack[sp + 1]) >= 1e-10 { stack[sp] = 1.0; } else { stack[sp] = 0.0; }
                sp = sp + 1;
            }
            default: {
                // Unknown opcode: treat as 0 and keep going so we don't hang.
            }
        }
    }

    if sp > 0 {
        return stack[sp - 1];
    }
    return 0.0;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.n {
        return;
    }

    let n = f32(max(params.n - 1u, 1u));
    let x = params.x_min + f32(gid.x) * (params.x_max - params.x_min) / n;
    values[gid.x] = eval_bytecode(x);
}
