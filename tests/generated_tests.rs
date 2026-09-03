//! Combinatorially generated fixtures.
//!
//! `tests/format_tests.rs` checks a handful of hand-picked snippets against
//! an expected output. This file instead builds many small snippets by
//! combining a short vocabulary (types, widths, names, comments, counts) and
//! checks each one against the actual formatter for the invariants that must
//! hold for any input, rather than a hand-computed expected string: it
//! doesn't error, it is idempotent, and it never changes the token stream.

use svfmt::{format_default, token_stream};

/// Format `source`, then check the invariants that must hold for any input.
fn check(source: &str) {
    let once = format_default(source).unwrap_or_else(|e| panic!("formatting failed on:\n{source}\nerror: {e}"));
    let twice = format_default(&once).unwrap_or_else(|e| panic!("second pass failed on:\n{once}\nerror: {e}"));
    assert_eq!(twice, once, "not idempotent for:\n{source}\nfirst pass:\n{once}");
    assert_eq!(
        token_stream(&once),
        token_stream(source),
        "token stream changed for:\n{source}\nformatted:\n{once}"
    );
    for line in once.lines() {
        assert_eq!(
            line,
            line.trim_end(),
            "trailing whitespace in output for:\n{source}\nformatted:\n{once}"
        );
    }
}

#[test]
fn generated_declaration_lists() {
    let types = ["logic", "wire", "reg", "bit", "int"];
    let widths = ["", "[7:0] ", "[WIDTH-1:0] "];
    let names = ["a", "bb", "ccc"];
    let inits = ["", " = '0", " = 1'b0"];
    let comments = ["", " // note"];

    for &ty in &types {
        for &width in &widths {
            for &init in &inits {
                for &comment in &comments {
                    let mut src = String::from("module top;\n");
                    for &name in &names {
                        src.push_str(&format!("  {ty} {width}{name}{init};{comment}\n"));
                    }
                    src.push_str("endmodule\n");
                    check(&src);
                }
            }
        }
    }
}

#[test]
fn generated_parameter_lists() {
    let types = ["int", "real", "logic", "bit"];
    let defaults = ["0", "1", "0.5", "1'b0"];
    let comments = ["", " // note"];
    let counts = [2, 3, 4];

    for &comment in &comments {
        for &count in &counts {
            let mut src = String::from("module top #(\n");
            for i in 0..count {
                let ty = types[i % types.len()];
                let default = defaults[i % defaults.len()];
                let name = format!("PARAM_{}", (b'A' + i as u8) as char);
                let sep = if i + 1 == count { "" } else { "," };
                src.push_str(&format!("  parameter {ty} {name} = {default}{sep}{comment}\n"));
            }
            src.push_str(") (\n  input logic clk\n);\nendmodule\n");
            check(&src);
        }
    }
}

#[test]
fn generated_port_lists() {
    let dirs = ["input", "output", "inout"];
    let types = ["wire", "logic", "reg"];
    let widths = ["", "[7:0] ", "[WIDTH-1:0] "];
    let names = ["a", "bb", "ccc"];

    for &dir in &dirs {
        for &ty in &types {
            for &width in &widths {
                let mut src = String::from("module top (\n");
                for (i, &name) in names.iter().enumerate() {
                    let sep = if i + 1 == names.len() { "" } else { "," };
                    src.push_str(&format!("  {dir} {ty} {width}{name}{sep}\n"));
                }
                src.push_str(");\nendmodule\n");
                check(&src);
            }
        }
    }
}

#[test]
fn generated_assignment_blocks() {
    let ops = ["=", "<="];
    let lhs_names = ["a", "bb", "some_longer_name"];
    let rhs_values = ["b", "b + 1", "'0"];

    for &op in &ops {
        for &rhs in &rhs_values {
            let mut src = String::from("module top;\n  always_comb begin\n");
            for &name in &lhs_names {
                src.push_str(&format!("    {name} {op} {rhs};\n"));
            }
            src.push_str("  end\nendmodule\n");
            check(&src);
        }
    }
}

#[test]
fn generated_instance_port_maps() {
    let port_name_sets = [["clk", "en", "q"], ["a_i", "b_i", "c_o"], ["x", "yy", "zzz"]];
    let exprs = ["clk", "some_signal", "1'b0"];
    let comments = ["", " // note"];

    for &names in &port_name_sets {
        for &comment in &comments {
            let mut src = String::from("module top;\n  leaf u_leaf (\n");
            for (i, name) in names.iter().enumerate() {
                let sep = if i + 1 == names.len() { "" } else { "," };
                let expr = exprs[i % exprs.len()];
                src.push_str(&format!("    .{name} ({expr}){sep}{comment}\n"));
            }
            src.push_str("  );\nendmodule\n");
            check(&src);
        }
    }
}

#[test]
fn generated_case_items() {
    let label_sets = [["2'b00", "2'b01", "default"], ["A", "B", "default"]];

    for &labels in &label_sets {
        let mut src = String::from("module top;\n  always_comb begin\n    case (sel)\n");
        for label in labels {
            src.push_str(&format!("    {label}: y = 0;\n"));
        }
        src.push_str("    endcase\n  end\nendmodule\n");
        check(&src);
    }
}

#[test]
fn generated_struct_members() {
    let widths = ["", "[7:0] ", "[N-1:0] "];
    let names = ["a", "bb", "ccc"];

    for &width in &widths {
        let mut src = String::from("module top;\n  typedef struct packed {\n");
        for &name in &names {
            src.push_str(&format!("    logic {width}{name};\n"));
        }
        src.push_str("  } struct_t;\nendmodule\n");
        check(&src);
    }
}

#[test]
fn generated_nested_blocks() {
    for depth in 1..=4 {
        let mut src = String::from("module top;\n  always_comb begin\n");
        for d in 0..depth {
            src.push_str(&"  ".repeat(d + 2));
            src.push_str("if (a) begin\n");
        }
        src.push_str(&"  ".repeat(depth + 2));
        src.push_str("x = 1;\n");
        for d in (0..depth).rev() {
            src.push_str(&"  ".repeat(d + 2));
            src.push_str("end\n");
        }
        src.push_str("  end\nendmodule\n");
        check(&src);
    }
}

#[test]
fn generated_multi_name_declarations() {
    let types = ["logic", "wire", "int"];
    let widths = ["", "[3:0] ", "[WIDTH-1:0] "];
    let name_groups: [&[&str]; 3] = [&["a", "b"], &["a", "b", "c"], &["x", "yy", "zzz", "w"]];

    for &ty in &types {
        for &width in &widths {
            for names in &name_groups {
                let mut src = String::from("module top;\n");
                src.push_str(&format!("  {ty} {width}{};\n", names.join(", ")));
                src.push_str("  logic other;\n");
                src.push_str("endmodule\n");
                check(&src);
            }
        }
    }
}

/// An inline attribute must not pull the padding of neighbouring lines that
/// lack one, for every arrangement of which lines carry one.
#[test]
fn generated_attributed_declarations() {
    let attrs = ["(* keep *) ", "(* unused *) ", ""];
    let names = ["a", "bb", "ccc"];

    for &first in &attrs {
        for &second in &attrs {
            for &third in &attrs {
                let combo = [first, second, third];
                let mut src = String::from("module top;\n");
                for (i, &name) in names.iter().enumerate() {
                    src.push_str(&format!("  {}logic {name} = 1'b0;\n", combo[i]));
                }
                src.push_str("endmodule\n");
                check(&src);
            }
        }
    }
}

#[test]
fn generated_unpacked_ports() {
    let dims = ["[N]", "[4]", "[N-1:0]"];
    let names = ["p", "q", "r"];

    for &dim in &dims {
        let mut src = String::from("module top (\n");
        for (i, &name) in names.iter().enumerate() {
            let sep = if i + 1 == names.len() { "" } else { "," };
            src.push_str(&format!("  input wire [7:0] {name} {dim}{sep}\n"));
        }
        src.push_str(");\nendmodule\n");
        check(&src);
    }
}

/// A hand-aligned ternary continuation must never be mistaken for a case
/// label, whatever the condition and branch expressions look like.
#[test]
fn generated_ternary_assignments() {
    let conditions = ["sel", "a == b", "en"];
    let true_vals = ["x", "1'b1", "'0"];
    let false_vals = ["y", "1'b0", "'1"];

    for &cond in &conditions {
        for &tv in &true_vals {
            for &fv in &false_vals {
                let src = format!("module top;\n  wire z = {cond} ? {tv} : {fv};\nendmodule\n");
                check(&src);
            }
        }
    }
}

#[test]
fn generated_if_else_chains() {
    for branches in 1..=4 {
        let mut src = String::from("module top;\n  always_comb begin\n");
        for b in 0..branches {
            let kw = if b == 0 { "if (a)" } else { "else if (a)" };
            src.push_str(&format!("    {kw}\n      x = {b};\n"));
        }
        src.push_str("    else\n      x = 0;\n");
        src.push_str("  end\nendmodule\n");
        check(&src);
    }
}

#[test]
fn generated_localparams() {
    let types = ["int", "logic", "real"];
    let counts = [2, 3, 4];

    for &ty in &types {
        for &count in &counts {
            let mut src = String::from("module top;\n");
            for i in 0..count {
                let name = format!("PARAM_{}", (b'A' + i as u8) as char);
                src.push_str(&format!("  localparam {ty} {name} = {i};\n"));
            }
            src.push_str("endmodule\n");
            check(&src);
        }
    }
}

#[test]
fn generated_enum_members() {
    let member_sets: [&[&str]; 3] = [&["A", "B"], &["A", "B", "C"], &["IDLE", "RUN", "DONE", "ERROR"]];

    for members in &member_sets {
        let mut src = String::from("module top;\n  typedef enum logic [1:0] {\n");
        for (i, m) in members.iter().enumerate() {
            let sep = if i + 1 == members.len() { "" } else { "," };
            src.push_str(&format!("    {m}{sep}\n"));
        }
        src.push_str("  } state_t;\nendmodule\n");
        check(&src);
    }
}

/// A blank line splits declarations into separate alignment runs, so a wider
/// block below does not push out a narrower one above it (or vice versa).
#[test]
fn generated_blank_separated_blocks() {
    let widths_a = ["[3:0]", "[7:0]"];
    let widths_b = ["[1:0]", "[15:0]"];

    for &wa in &widths_a {
        for &wb in &widths_b {
            let src = format!("module top;\n  logic {wa} a;\n  logic {wa} bb;\n\n  logic {wb} c;\n  logic {wb} dd;\nendmodule\n");
            check(&src);
        }
    }
}
