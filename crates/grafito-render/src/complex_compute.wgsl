// Complex arithmetic represented as 2x2 matrices
// Z = [[x, -y], [y, x]]
// In WGSL, mat2x2<f32>(vec2(x, y), vec2(-y, x)) where first vec2 is column 0 and second is column 1.

fn c_new(x: f32, y: f32) -> mat2x2<f32> {
    return mat2x2<f32>(vec2<f32>(x, y), vec2<f32>(-y, x));
}

fn c_real(z: mat2x2<f32>) -> f32 {
    return z[0][0];
}

fn c_imag(z: mat2x2<f32>) -> f32 {
    return z[0][1];
}

fn c_mul(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a * b; // Native matrix multiplication
}

fn c_div(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    let det = b[0][0]*b[1][1] - b[0][1]*b[1][0];
    if abs(det) < 1e-15 {
        return c_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
    }
    let inv_det = 1.0 / det;
    let inv_b = mat2x2<f32>(
        vec2<f32>(b[1][1] * inv_det, -b[0][1] * inv_det),
        vec2<f32>(-b[1][0] * inv_det, b[0][0] * inv_det)
    );
    return a * inv_b;
}

fn c_exp(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    let ex = exp(x);
    return c_new(ex * cos(y), ex * sin(y));
}

fn c_log(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    let r = sqrt(x*x + y*y);
    if r < 1e-15 {
        return c_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
    }
    let theta = atan2(y, x);
    return c_new(log(r), theta);
}

fn c_pow(base: mat2x2<f32>, exponent: mat2x2<f32>) -> mat2x2<f32> {
    // If base is real positive and exponent is real
    if abs(c_imag(base)) < 1e-7 && c_real(base) >= 0.0 && abs(c_imag(exponent)) < 1e-7 {
        return c_new(pow(c_real(base), c_real(exponent)), 0.0);
    }
    let ln_b = c_log(base);
    // If ln_b is NaN, propagate
    if c_real(ln_b) != c_real(ln_b) {
        return ln_b;
    }
    let p = c_mul(exponent, ln_b);
    return c_exp(p);
}

fn c_sin(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    return c_new(sin(x)*cosh(y), cos(x)*sinh(y));
}

fn c_cos(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    return c_new(cos(x)*cosh(y), -sin(x)*sinh(y));
}

fn c_tan(z: mat2x2<f32>) -> mat2x2<f32> {
    let num = c_sin(z);
    let den = c_cos(z);
    return c_div(num, den);
}

fn c_sinh(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    return c_new(sinh(x)*cos(y), cosh(x)*sin(y));
}

fn c_cosh(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    return c_new(cosh(x)*cos(y), sinh(x)*sin(y));
}

fn c_sqrt(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = c_real(z);
    let y = c_imag(z);
    let r = sqrt(x*x + y*y);
    let cx = sqrt((r + x) * 0.5);
    var cy = sqrt((r - x) * 0.5);
    if y < 0.0 { cy = -cy; }
    return c_new(cx, cy);
}

fn c_abs(z: mat2x2<f32>) -> mat2x2<f32> {
    let r = sqrt(c_real(z)*c_real(z) + c_imag(z)*c_imag(z));
    return c_new(r, 0.0);
}

fn c_conj(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(c_real(z), -c_imag(z));
}

fn c_re(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(c_real(z), 0.0);
}

fn c_im(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(c_imag(z), 0.0);
}

fn c_arg(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(atan2(c_imag(z), c_real(z)), 0.0);
}

fn c_min(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(min(c_real(a), c_real(b)), min(c_imag(a), c_imag(b)));
}

fn c_max(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(max(c_real(a), c_real(b)), max(c_imag(a), c_imag(b)));
}

fn c_floor(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(floor(c_real(z)), floor(c_imag(z)));
}

fn c_ceil(z: mat2x2<f32>) -> mat2x2<f32> {
    return c_new(ceil(c_real(z)), ceil(c_imag(z)));
}

fn c_mod(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    // Only strictly well-defined for reals in WGSL context usually
    let v = c_real(a) - c_real(b) * floor(c_real(a) / c_real(b));
    return c_new(v, 0.0);
}

// Relational operations return 1.0 or 0.0 mapped to real part of complex
fn c_eq(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    if abs(c_real(a) - c_real(b)) < 1e-10 && abs(c_imag(a) - c_imag(b)) < 1e-10 {
        return c_new(1.0, 0.0);
    }
    return c_new(0.0, 0.0);
}

fn c_lt(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    if c_real(a) < c_real(b) {
        return c_new(1.0, 0.0);
    }
    return c_new(0.0, 0.0);
}

fn c_asinh(z: mat2x2<f32>) -> mat2x2<f32> {
    let one = c_new(1.0, 0.0);
    let z2 = c_mul(z, z);
    let sqrt_term = c_sqrt(c_add(z2, one));
    return c_log(c_add(z, sqrt_term));
}

fn c_acosh(z: mat2x2<f32>) -> mat2x2<f32> {
    let one = c_new(1.0, 0.0);
    let zp1 = c_sqrt(c_add(z, one));
    let zm1 = c_sqrt(c_sub(z, one));
    return c_log(c_add(z, c_mul(zp1, zm1)));
}

fn c_atanh(z: mat2x2<f32>) -> mat2x2<f32> {
    let one = c_new(1.0, 0.0);
    let num = c_add(one, z);
    let den = c_sub(one, z);
    let half = c_new(0.5, 0.0);
    return c_mul(half, c_log(c_div(num, den)));
}

struct TransformParams {
    vertex_count: u32,
    code_len: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0)
var<uniform> params: TransformParams;

@group(0) @binding(1)
var<storage, read> bytecode: array<u32>;

@group(0) @binding(2)
var<storage, read> constants: array<vec2<f32>>;

@group(0) @binding(3)
var<storage, read> in_vertices: array<vec2<f32>>;

@group(0) @binding(4)
var<storage, read_write> out_vertices: array<vec2<f32>>;

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
const OP_LOG: u32 = 13u;
const OP_SQRT: u32 = 14u;
const OP_ABS: u32 = 15u;
const OP_MIN: u32 = 16u;
const OP_MAX: u32 = 17u;
const OP_FLOOR: u32 = 18u;
const OP_CEIL: u32 = 19u;
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
const OP_CONJUGATE: u32 = 102u;
const OP_REAL_PART: u32 = 103u;
const OP_IMAG_PART: u32 = 104u;
const OP_ARG: u32 = 105u;
const OP_ERF: u32 = 106u;
const OP_LAMBERTW: u32 = 107u;
const OP_ZETA: u32 = 108u;
const OP_BESSELY: u32 = 109u;

const STACK_SIZE: i32 = 32;

fn c_add(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a + b;
}

fn c_sub(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a - b;
}

fn c_neg(a: mat2x2<f32>) -> mat2x2<f32> {
    return mat2x2<f32>(vec2<f32>(-a[0][0], -a[0][1]), vec2<f32>(-a[1][0], -a[1][1]));
}

fn eval_bytecode(z: mat2x2<f32>, x: mat2x2<f32>, y: mat2x2<f32>) -> mat2x2<f32> {
    var stack: array<mat2x2<f32>, STACK_SIZE>;
    var sp: i32 = 0;

    let len = params.code_len;
    for (var pc: u32 = 0u; pc < len; pc = pc + 1u) {
        if sp < 0 || sp >= STACK_SIZE {
            return c_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
        }
        let instr = bytecode[pc];
        let op = instr & 0xFFu;
        let operand = instr >> 8u;

        switch op {
            case OP_PUSH_CONST: {
                let c = constants[operand];
                stack[sp] = c_new(c.x, c.y);
                sp = sp + 1;
            }
            case OP_PUSH_VAR: {
                if operand == 0u {
                    stack[sp] = z;
                } else if operand == 1u {
                    stack[sp] = x;
                } else {
                    stack[sp] = y;
                }
                sp = sp + 1;
            }
            case OP_ADD: {
                sp = sp - 2;
                stack[sp] = c_add(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_SUB: {
                sp = sp - 2;
                stack[sp] = c_sub(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_MUL: {
                sp = sp - 2;
                stack[sp] = c_mul(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_DIV: {
                sp = sp - 2;
                stack[sp] = c_div(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_POW: {
                sp = sp - 2;
                stack[sp] = c_pow(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_NEG: {
                sp = sp - 1;
                stack[sp] = c_neg(stack[sp]);
                sp = sp + 1;
            }
            case OP_SIN: {
                sp = sp - 1;
                stack[sp] = c_sin(stack[sp]);
                sp = sp + 1;
            }
            case OP_COS: {
                sp = sp - 1;
                stack[sp] = c_cos(stack[sp]);
                sp = sp + 1;
            }
            case OP_TAN: {
                sp = sp - 1;
                stack[sp] = c_tan(stack[sp]);
                sp = sp + 1;
            }
            case OP_EXP: {
                sp = sp - 1;
                stack[sp] = c_exp(stack[sp]);
                sp = sp + 1;
            }
            case OP_LOG: {
                sp = sp - 1;
                stack[sp] = c_log(stack[sp]);
                sp = sp + 1;
            }
            case OP_SQRT: {
                sp = sp - 1;
                stack[sp] = c_sqrt(stack[sp]);
                sp = sp + 1;
            }
            case OP_ABS: {
                sp = sp - 1;
                stack[sp] = c_abs(stack[sp]);
                sp = sp + 1;
            }
            case OP_MIN: {
                sp = sp - 2;
                stack[sp] = c_min(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_MAX: {
                sp = sp - 2;
                stack[sp] = c_max(stack[sp], stack[sp + 1]);
                sp = sp + 1;
            }
            case OP_FLOOR: {
                sp = sp - 1;
                stack[sp] = c_floor(stack[sp]);
                sp = sp + 1;
            }
            case OP_CEIL: {
                sp = sp - 1;
                stack[sp] = c_ceil(stack[sp]);
                sp = sp + 1;
            }
            case OP_ASINH: {
                sp = sp - 1;
                stack[sp] = c_asinh(stack[sp]);
                sp = sp + 1;
            }
            case OP_ACOSH: {
                sp = sp - 1;
                stack[sp] = c_acosh(stack[sp]);
                sp = sp + 1;
            }
            case OP_ATANH: {
                sp = sp - 1;
                stack[sp] = c_atanh(stack[sp]);
                sp = sp + 1;
            }
            case OP_SEC: {
                sp = sp - 1;
                let one = c_new(1.0, 0.0);
                stack[sp] = c_div(one, c_cos(stack[sp]));
                sp = sp + 1;
            }
            case OP_CSC: {
                sp = sp - 1;
                let one = c_new(1.0, 0.0);
                stack[sp] = c_div(one, c_sin(stack[sp]));
                sp = sp + 1;
            }
            case OP_COT: {
                sp = sp - 1;
                let one = c_new(1.0, 0.0);
                stack[sp] = c_div(one, c_tan(stack[sp]));
                sp = sp + 1;
            }
            // For other inverses, they might not be fully implemented in WGSL yet, return NaN for now
            // or implement them using logs. We already added Asinh, Acosh, Atanh above!
            // Wait, we need Asin, Acos, Atan!
            case OP_ASIN: {
                sp = sp - 1;
                // asin(z) = -i * ln(i*z + sqrt(1 - z^2))
                let i = c_new(0.0, 1.0);
                let z = stack[sp];
                let z2 = c_mul(z, z);
                let one = c_new(1.0, 0.0);
                let sqrt_term = c_sqrt(c_sub(one, z2));
                let iz = c_mul(i, z);
                stack[sp] = c_mul(c_new(0.0, -1.0), c_log(c_add(iz, sqrt_term)));
                sp = sp + 1;
            }
            case OP_ACOS: {
                sp = sp - 1;
                // acos(z) = -i * ln(z + i*sqrt(1 - z^2))
                let i = c_new(0.0, 1.0);
                let z = stack[sp];
                let z2 = c_mul(z, z);
                let one = c_new(1.0, 0.0);
                let sqrt_term = c_sqrt(c_sub(one, z2));
                let i_sqrt = c_mul(i, sqrt_term);
                stack[sp] = c_mul(c_new(0.0, -1.0), c_log(c_add(z, i_sqrt)));
                sp = sp + 1;
            }
            case OP_ATAN: {
                sp = sp - 1;
                // atan(z) = (i/2) * ln((i+z)/(i-z))
                let i = c_new(0.0, 1.0);
                let z = stack[sp];
                let num = c_add(i, z);
                let den = c_sub(i, z);
                stack[sp] = c_mul(c_new(0.0, 0.5), c_log(c_div(num, den)));
                sp = sp + 1;
            }
            case OP_CONJUGATE: {
                sp = sp - 1;
                stack[sp] = c_conj(stack[sp]);
                sp = sp + 1;
            }
            case OP_REAL_PART: {
                sp = sp - 1;
                stack[sp] = c_re(stack[sp]);
                sp = sp + 1;
            }
            case OP_IMAG_PART: {
                sp = sp - 1;
                stack[sp] = c_im(stack[sp]);
                sp = sp + 1;
            }
            case OP_ARG: {
                sp = sp - 1;
                stack[sp] = c_arg(stack[sp]);
                sp = sp + 1;
            }
            // OP_ERF, OP_LAMBERTW, OP_ZETA, OP_BESSELY: CPU-only functions.
            // GPU returns NaN; CPU fallback handles these.
            default: {
                // Return NaN if opcode unsupported on GPU
                return c_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
            }
        }
    }

    if sp > 0 {
        return stack[sp - 1];
    }
    return c_new(0.0, 0.0);
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.vertex_count {
        return;
    }

    let in_pos = in_vertices[gid.x];
    if in_pos.x != in_pos.x {
        out_vertices[gid.x] = in_pos;
        return;
    }

    let z = c_new(in_pos.x, in_pos.y);
    let x = c_new(in_pos.x, 0.0);
    let y = c_new(in_pos.y, 0.0);

    let result = eval_bytecode(z, x, y);

    out_vertices[gid.x] = vec2<f32>(c_real(result), c_imag(result));
}
