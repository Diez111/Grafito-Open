// Grafito 2D vector-field compute evaluator.
//
// Interprets a small RPN bytecode stream to evaluate P(x,y) and Q(x,y) for
// every grid cell in parallel. The bytecode is the concatenation of the
// two expressions (u first, then v), so the stack ends with [..., u, v].
// The shader returns the two topmost values and writes (x, y, u, v).
//
// Bindings:
//   0 - uniform Params
//   1 - storage read bytecode (u32[])
//   2 - storage read constants (f32[])
//   3 - storage read_write values (f32[])

struct Params {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    nx: u32,
    ny: u32,
    code_len: u32,
    _pad0: u32,
};

@group(0) @binding(0)
var<uniform> params: Params;

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

const STACK_SIZE: i32 = 32;

struct EvalResult {
    u: f32,
    v: f32,
};

fn eval_bytecode(x: f32, y: f32) -> EvalResult {
    var stack: array<f32, STACK_SIZE>;
    var sp: i32 = 0;

    let len = params.code_len;
    for (var pc: u32 = 0u; pc < len; pc = pc + 1u) {
        let instr = bytecode[pc];
        let op = instr & 0xFFu;
        let operand = instr >> 8u;

        switch op {
            case OP_PUSH_CONST: {
                stack[sp] = constants[operand];
                sp = sp + 1;
            }
            case OP_PUSH_VAR: {
                if operand == 0u {
                    stack[sp] = x;
                } else {
                    stack[sp] = y;
                }
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
                stack[sp] = select(log(v), 0.0, v <= 0.0);
                sp = sp + 1;
            }
            case OP_SQRT: {
                sp = sp - 1;
                let v = stack[sp];
                stack[sp] = select(sqrt(v), 0.0, v < 0.0);
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
            default: {
                // Unknown opcode: treat as 0 and keep going so we don't hang.
            }
        }
    }

    var res = EvalResult(0.0, 0.0);
    if sp > 0 {
        res.v = stack[sp - 1];
    }
    if sp > 1 {
        res.u = stack[sp - 2];
    }
    return res;
}

@compute @workgroup_size(16, 16)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.nx || gid.y >= params.ny {
        return;
    }

    let nx = f32(params.nx - 1u);
    let ny = f32(params.ny - 1u);
    let x = params.x_min + f32(gid.x) * (params.x_max - params.x_min) / nx;
    let y = params.y_min + f32(gid.y) * (params.y_max - params.y_min) / ny;

    let r = eval_bytecode(x, y);
    let idx = (gid.y * (params.nx + 1u) + gid.x) * 4u;
    values[idx] = x;
    values[idx + 1u] = y;
    values[idx + 2u] = r.u;
    values[idx + 3u] = r.v;
}
