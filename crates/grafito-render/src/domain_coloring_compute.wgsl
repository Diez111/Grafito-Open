// Domain coloring GPU compute shader.
// Evaluates f(z) on a 2D grid and produces RGBA colors using HSL domain coloring.
// Hue = arg(f(z)), Lightness = atan(ln(|f(z)|))/(pi/2) * 0.5 + 0.5

fn dc_new(x: f32, y: f32) -> mat2x2<f32> {
    return mat2x2<f32>(vec2<f32>(x, y), vec2<f32>(-y, x));
}

fn dc_real(z: mat2x2<f32>) -> f32 {
    return z[0][0];
}

fn dc_imag(z: mat2x2<f32>) -> f32 {
    return z[0][1];
}

fn dc_mul(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a * b;
}

fn dc_div(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    let det = b[0][0]*b[1][1] - b[0][1]*b[1][0];
    if abs(det) < 1e-15 {
        return dc_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
    }
    let inv_det = 1.0 / det;
    let inv_b = mat2x2<f32>(
        vec2<f32>(b[1][1] * inv_det, -b[0][1] * inv_det),
        vec2<f32>(-b[1][0] * inv_det, b[0][0] * inv_det)
    );
    return a * inv_b;
}

fn dc_exp(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    let ex = exp(x);
    return dc_new(ex * cos(y), ex * sin(y));
}

fn dc_log(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    let r = sqrt(x*x + y*y);
    if r < 1e-15 {
        return dc_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
    }
    let theta = atan2(y, x);
    return dc_new(log(r), theta);
}

fn dc_pow(base: mat2x2<f32>, exponent: mat2x2<f32>) -> mat2x2<f32> {
    if abs(dc_imag(base)) < 1e-7 && dc_real(base) >= 0.0 && abs(dc_imag(exponent)) < 1e-7 {
        return dc_new(pow(dc_real(base), dc_real(exponent)), 0.0);
    }
    let ln_b = dc_log(base);
    if dc_real(ln_b) != dc_real(ln_b) {
        return ln_b;
    }
    let p = dc_mul(exponent, ln_b);
    return dc_exp(p);
}

fn dc_sin(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    return dc_new(sin(x)*cosh(y), cos(x)*sinh(y));
}

fn dc_cos(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    return dc_new(cos(x)*cosh(y), -sin(x)*sinh(y));
}

fn dc_tan(z: mat2x2<f32>) -> mat2x2<f32> {
    return dc_div(dc_sin(z), dc_cos(z));
}

fn dc_sinh(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    return dc_new(sinh(x)*cos(y), cosh(x)*sin(y));
}

fn dc_cosh(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    return dc_new(cosh(x)*cos(y), sinh(x)*sin(y));
}

fn dc_sqrt(z: mat2x2<f32>) -> mat2x2<f32> {
    let x = dc_real(z);
    let y = dc_imag(z);
    let r = sqrt(x*x + y*y);
    let cx = sqrt((r + x) * 0.5);
    var cy = sqrt((r - x) * 0.5);
    if y < 0.0 { cy = -cy; }
    return dc_new(cx, cy);
}

fn dc_abs(z: mat2x2<f32>) -> mat2x2<f32> {
    let r = sqrt(dc_real(z)*dc_real(z) + dc_imag(z)*dc_imag(z));
    return dc_new(r, 0.0);
}

fn dc_add(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a + b;
}

fn dc_sub(a: mat2x2<f32>, b: mat2x2<f32>) -> mat2x2<f32> {
    return a - b;
}

fn dc_neg(a: mat2x2<f32>) -> mat2x2<f32> {
    return mat2x2<f32>(vec2<f32>(-a[0][0], -a[0][1]), vec2<f32>(-a[1][0], -a[1][1]));
}

fn dc_conj(z: mat2x2<f32>) -> mat2x2<f32> {
    return dc_new(dc_real(z), -dc_imag(z));
}

// HSL to RGB conversion (matches CPU implementation)
fn hue_to_rgb_fn(p: f32, q: f32, t: f32) -> f32 {
    var tt = t;
    if tt < 0.0 { tt = tt + 1.0; }
    if tt > 1.0 { tt = tt - 1.0; }
    if tt < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * tt;
    }
    if tt < 1.0 / 2.0 {
        return q;
    }
    if tt < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - tt) * 6.0;
    }
    return p;
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3<f32> {
    if s == 0.0 {
        return vec3<f32>(l, l, l);
    }
    var q: f32;
    if l < 0.5 {
        q = l * (1.0 + s);
    } else {
        q = l + s - l * s;
    }
    let p = 2.0 * l - q;
    let r = hue_to_rgb_fn(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb_fn(p, q, h);
    let b = hue_to_rgb_fn(p, q, h - 1.0 / 3.0);
    return vec3<f32>(r, g, b);
}

struct GridParams {
    grid_size: u32,
    code_len: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0)
var<uniform> params: GridParams;

@group(0) @binding(1)
var<storage, read> bytecode: array<u32>;

@group(0) @binding(2)
var<storage, read> constants: array<vec2<f32>>;

@group(0) @binding(3)
var<storage, read> in_points: array<vec2<f32>>;

@group(0) @binding(4)
var<storage, read_write> out_colors: array<vec4<f32>>;

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
const OP_ASIN: u32 = 22u;
const OP_ACOS: u32 = 23u;
const OP_ATAN: u32 = 24u;
const OP_SINH: u32 = 25u;
const OP_COSH: u32 = 26u;
const OP_TANH: u32 = 27u;
const OP_SEC: u32 = 31u;
const OP_CSC: u32 = 32u;
const OP_COT: u32 = 33u;
const OP_CONJUGATE: u32 = 102u;
const OP_REAL_PART: u32 = 103u;
const OP_IMAG_PART: u32 = 104u;
const OP_ARG: u32 = 105u;

const STACK_SIZE: i32 = 32;

fn eval_bytecode_dc(z: mat2x2<f32>) -> mat2x2<f32> {
    var stack: array<mat2x2<f32>, STACK_SIZE>;
    var sp: i32 = 0;

    let x = dc_new(dc_real(z), 0.0);
    let y = dc_new(dc_imag(z), 0.0);

    let len = params.code_len;
    for (var pc: u32 = 0u; pc < len; pc = pc + 1u) {
        if sp < 0 || sp >= STACK_SIZE {
            return dc_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
        }
        let instr = bytecode[pc];
        let op = instr & 0xFFu;
        let operand = instr >> 8u;

        switch op {
            case OP_PUSH_CONST: {
                let c = constants[operand];
                stack[sp] = dc_new(c.x, c.y);
                sp = sp + 1;
            }
            case OP_PUSH_VAR: {
                if operand == 0u { stack[sp] = z; }
                else if operand == 1u { stack[sp] = x; }
                else { stack[sp] = y; }
                sp = sp + 1;
            }
            case OP_ADD: { sp = sp - 2; stack[sp] = dc_add(stack[sp], stack[sp + 1]); sp = sp + 1; }
            case OP_SUB: { sp = sp - 2; stack[sp] = dc_sub(stack[sp], stack[sp + 1]); sp = sp + 1; }
            case OP_MUL: { sp = sp - 2; stack[sp] = dc_mul(stack[sp], stack[sp + 1]); sp = sp + 1; }
            case OP_DIV: { sp = sp - 2; stack[sp] = dc_div(stack[sp], stack[sp + 1]); sp = sp + 1; }
            case OP_POW: { sp = sp - 2; stack[sp] = dc_pow(stack[sp], stack[sp + 1]); sp = sp + 1; }
            case OP_NEG: { sp = sp - 1; stack[sp] = dc_neg(stack[sp]); sp = sp + 1; }
            case OP_SIN: { sp = sp - 1; stack[sp] = dc_sin(stack[sp]); sp = sp + 1; }
            case OP_COS: { sp = sp - 1; stack[sp] = dc_cos(stack[sp]); sp = sp + 1; }
            case OP_TAN: { sp = sp - 1; stack[sp] = dc_tan(stack[sp]); sp = sp + 1; }
            case OP_EXP: { sp = sp - 1; stack[sp] = dc_exp(stack[sp]); sp = sp + 1; }
            case OP_LOG: { sp = sp - 1; stack[sp] = dc_log(stack[sp]); sp = sp + 1; }
            case OP_SQRT: { sp = sp - 1; stack[sp] = dc_sqrt(stack[sp]); sp = sp + 1; }
            case OP_ABS: { sp = sp - 1; stack[sp] = dc_abs(stack[sp]); sp = sp + 1; }
            case OP_CONJUGATE: { sp = sp - 1; stack[sp] = dc_conj(stack[sp]); sp = sp + 1; }
            case OP_REAL_PART: { sp = sp - 1; stack[sp] = dc_new(dc_real(stack[sp]), 0.0); sp = sp + 1; }
            case OP_IMAG_PART: { sp = sp - 1; stack[sp] = dc_new(dc_imag(stack[sp]), 0.0); sp = sp + 1; }
            case OP_ARG: {
                sp = sp - 1;
                let r = dc_real(stack[sp]);
                let i = dc_imag(stack[sp]);
                stack[sp] = dc_new(atan2(i, r), 0.0);
                sp = sp + 1;
            }
            case OP_SEC: { sp = sp - 1; let one = dc_new(1.0, 0.0); stack[sp] = dc_div(one, dc_cos(stack[sp])); sp = sp + 1; }
            case OP_CSC: { sp = sp - 1; let one = dc_new(1.0, 0.0); stack[sp] = dc_div(one, dc_sin(stack[sp])); sp = sp + 1; }
            case OP_COT: { sp = sp - 1; let one = dc_new(1.0, 0.0); stack[sp] = dc_div(one, dc_tan(stack[sp])); sp = sp + 1; }
            case OP_SINH: { sp = sp - 1; stack[sp] = dc_sinh(stack[sp]); sp = sp + 1; }
            case OP_COSH: { sp = sp - 1; stack[sp] = dc_cosh(stack[sp]); sp = sp + 1; }
            case OP_TANH: {
                sp = sp - 1;
                let sh = dc_sinh(stack[sp]);
                let ch = dc_cosh(stack[sp]);
                stack[sp] = dc_div(sh, ch);
                sp = sp + 1;
            }
            case OP_ASIN: {
                sp = sp - 1;
                let i = dc_new(0.0, 1.0);
                let z2 = dc_mul(stack[sp], stack[sp]);
                let one = dc_new(1.0, 0.0);
                let sqrt_term = dc_sqrt(dc_sub(one, z2));
                let iz = dc_mul(i, stack[sp]);
                stack[sp] = dc_mul(dc_new(0.0, -1.0), dc_log(dc_add(iz, sqrt_term)));
                sp = sp + 1;
            }
            case OP_ACOS: {
                sp = sp - 1;
                let i = dc_new(0.0, 1.0);
                let z2 = dc_mul(stack[sp], stack[sp]);
                let one = dc_new(1.0, 0.0);
                let sqrt_term = dc_sqrt(dc_sub(one, z2));
                let i_sqrt = dc_mul(i, sqrt_term);
                stack[sp] = dc_mul(dc_new(0.0, -1.0), dc_log(dc_add(stack[sp], i_sqrt)));
                sp = sp + 1;
            }
            case OP_ATAN: {
                sp = sp - 1;
                let i = dc_new(0.0, 1.0);
                let num = dc_add(i, stack[sp]);
                let den = dc_sub(i, stack[sp]);
                stack[sp] = dc_mul(dc_new(0.0, 0.5), dc_log(dc_div(num, den)));
                sp = sp + 1;
            }
            default: {
                return dc_new(bitcast<f32>(0x7fc00000u), bitcast<f32>(0x7fc00000u));
            }
        }
    }

    if sp > 0 { return stack[sp - 1]; }
    return dc_new(0.0, 0.0);
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.grid_size {
        return;
    }

    let in_pos = in_points[gid.x];
    let z = dc_new(in_pos.x, in_pos.y);
    let result = eval_bytecode_dc(z);

    let re = dc_real(result);
    let im = dc_imag(result);

    if re != re || im != im {
        out_colors[gid.x] = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return;
    }

    let mag = sqrt(re*re + im*im);
    let arg = atan2(im, re);

    let hue = (arg + 3.14159265359) / (2.0 * 3.14159265359);
    var lightness = atan(log(max(mag, 1e-10))) / 1.57079632679 * 0.5 + 0.5;
    lightness = clamp(lightness, 0.0, 1.0);

    let rgb = hsl_to_rgb(hue, 0.85, lightness);
    out_colors[gid.x] = vec4<f32>(rgb, 1.0);
}
