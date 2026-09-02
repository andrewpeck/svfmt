//! Tests for the (System)Verilog whitespace formatter.
//!
//! Each fixture is a minimal, made-up snippet chosen only to exercise one
//! formatting rule; the expected output is what the formatter actually
//! produces for it.

use svfmt::{FormatOptions, SvFormatError, format_default, format_text, token_stream};

// ---------------------------------------------------------------------------------
// parameters: '=' column plus a trailing comment column
// ---------------------------------------------------------------------------------

const PARAMS_IN: &str = r#"module top #(
  parameter int PARAM_A = 3,
  parameter real PARAM_B = 0.05,
  parameter real PARAM_C = 3.5, // note one
  parameter real PARAM_D = 0.2, // note two
  parameter real PARAM_E = 0.1, // note three
  parameter logic PARAM_F = 0
) (
"#;

const PARAMS_OUT: &str = r#"module top #(
  parameter int   PARAM_A = 3,
  parameter real  PARAM_B = 0.05,
  parameter real  PARAM_C = 3.5, // note one
  parameter real  PARAM_D = 0.2, // note two
  parameter real  PARAM_E = 0.1, // note three
  parameter logic PARAM_F = 0
) (
"#;

// ---------------------------------------------------------------------------------
// module ports: packed and unpacked dimensions
// ---------------------------------------------------------------------------------

const PORTS_IN: &str = r#"module top (
  input wire clk,
  output logic [WIDTH+1:0] a_o,
  output logic [WIDTH-1:0] b_o
);
"#;

const PORTS_OUT: &str = r#"module top (
  input wire               clk,
  output logic [WIDTH+1:0] a_o,
  output logic [WIDTH-1:0] b_o
);
"#;

/// banner comments and blank lines inside a port list do not split the run,
/// and an unpacked dimension trails its name instead of forming a column
const HEADER_IN: &str = r#"module top (

  input wire clk,

  //------------------------------------------------------------------------------
  // Example Section
  //------------------------------------------------------------------------------

  input wire [A-1:0] p [N],
  input wire [5:0][B-1:0] q [N],
  output reg [C-1:0] r

);
"#;

const HEADER_OUT: &str = r#"module top (

  input wire              clk,

  //------------------------------------------------------------------------------
  // Example Section
  //------------------------------------------------------------------------------

  input wire [A-1:0]      p [N],
  input wire [5:0][B-1:0] q [N],
  output reg [C-1:0]      r

);
"#;

// ---------------------------------------------------------------------------------
// variable blocks: attributes, and blank lines separating independent blocks
// ---------------------------------------------------------------------------------

const VARIABLES_IN: &str = r#"module top;
  (* unused *) int unsigned a;
  (* unused *) int unsigned b;
  (* unused *) realtime c;
  (* unused *) realtime d;

  realtime e;
  realtime f;
endmodule
"#;

const VARIABLES_OUT: &str = r#"module top;
  (* unused *) int unsigned a;
  (* unused *) int unsigned b;
  (* unused *) realtime     c;
  (* unused *) realtime     d;

  realtime e;
  realtime f;
endmodule
"#;

const ATTRIBUTED_LIST_IN: &str = r#"module top;
  (* unused *) logic [1:0] a, b, c;
  (* keep = "true" *) logic [1:0] d = '1;
  (* keep = "true" *) logic [1:0] e = '1;
endmodule
"#;

const ATTRIBUTED_LIST_OUT: &str = r#"module top;
  (* unused *)        logic [1:0] a, b, c;
  (* keep = "true" *) logic [1:0] d = '1;
  (* keep = "true" *) logic [1:0] e = '1;
endmodule
"#;

/// a declaration without an initializer must not push out the '=' of a
/// neighbouring one that has one, but initialised declarations align together
const INITIALISER_IN: &str = r#"module top;
  (* max_fanout = 64 *)
  logic a;
  (* max_fanout = 4 *)
  logic b;
  localparam int DEPTH = 8;

  wire [3:0] c = d;
  wire [3:0] c_next = e;
endmodule
"#;

const INITIALISER_OUT: &str = r#"module top;
  (* max_fanout = 64 *)
  logic          a;
  (* max_fanout = 4 *)
  logic          b;
  localparam int DEPTH = 8;

  wire [3:0] c      = d;
  wire [3:0] c_next = e;
endmodule
"#;

// ---------------------------------------------------------------------------------
// struct members
// ---------------------------------------------------------------------------------

const STRUCT_IN: &str = r#"module top;
  typedef struct packed {
    logic a;
    logic [A-1:0] b;
    logic [B-1:0] c;
    logic [N-1:0][4:0] d;
  } struct_one_t;

  typedef struct packed {
    entry_t [N-1:0] e;
    select_t [N-1:0] f;
    logic [1:0] g;
  } struct_two_t;
endmodule
"#;

const STRUCT_OUT: &str = r#"module top;
  typedef struct packed {
    logic              a;
    logic [A-1:0]      b;
    logic [B-1:0]      c;
    logic [N-1:0][4:0] d;
  } struct_one_t;

  typedef struct packed {
    entry_t [N-1:0]  e;
    select_t [N-1:0] f;
    logic [1:0]      g;
  } struct_two_t;
endmodule
"#;

// ---------------------------------------------------------------------------------
// assignments
// ---------------------------------------------------------------------------------

const ASSIGN_IN: &str = r#"module top;
  always_comb begin
    x.a = a;
    x.b = b;
    x.some_longer_field = some_longer_field;
  end

  assign y.a = z.a;
  assign y.b = WIDTH'(w);
endmodule
"#;

const ASSIGN_OUT: &str = r#"module top;
  always_comb begin
    x.a                 = a;
    x.b                 = b;
    x.some_longer_field = some_longer_field;
  end

  assign y.a = z.a;
  assign y.b = WIDTH'(w);
endmodule
"#;

/// blocking and non-blocking assignments align on the assignment itself
const MIXED_OPS_IN: &str = r#"module top;
  always @(posedge clk) begin
    a = compute();
    b = b + 1;
    c <= value;
  end
endmodule
"#;

const MIXED_OPS_OUT: &str = r#"module top;
  always @(posedge clk) begin
    a  = compute();
    b  = b + 1;
    c <= value;
  end
endmodule
"#;

// ---------------------------------------------------------------------------------
// named port connections
// ---------------------------------------------------------------------------------

const PORTMAP_IN: &str = r#"module top;
  leaf u_leaf (
    .clk(clk),
    .index (some_index),

    /* example */
    .valid_i (some_condition), // FIXME: placeholder
    .result_o (results[i])
  );
endmodule
"#;

const PORTMAP_OUT: &str = r#"module top;
  leaf u_leaf (
    .clk      (clk),
    .index    (some_index),

    /* example */
    .valid_i  (some_condition), // FIXME: placeholder
    .result_o (results[i])
  );
endmodule
"#;

/// an instantiation split over lines puts the instance name at the indent of
/// the module name, and its port map one level in
const INSTANCE_IN: &str = r#"module top;
  block
    u_block (
      .a_i (a),
      .b_i (~b),
      .c_o (),
      .d_o (d));
endmodule
"#;

const INSTANCE_OUT: &str = r#"module top;
  block
  u_block (
    .a_i (a),
    .b_i (~b),
    .c_o (),
    .d_o (d));
endmodule
"#;

/// the same, with a parameter list closing on its own line
const INSTANCE_PARAMS_IN: &str = r#"module top;
  block_b #(
    .PARAM_A (1),
    .PARAM_B (N-1)
  )
      u_block_b (
    .clk (clk),
    .in0 (data_i),
    .out0 (data_o));
endmodule
"#;

const INSTANCE_PARAMS_OUT: &str = r#"module top;
  block_b #(
    .PARAM_A (1),
    .PARAM_B (N-1)
  )
  u_block_b (
    .clk  (clk),
    .in0  (data_i),
    .out0 (data_o));
endmodule
"#;

/// a port map that closes its parentheses on the last connection stays one run
const PORTMAP_TRAILING_IN: &str = r#"module top;
  block_b #(
    .PARAM_A (1),
    .PARAM_B (N-1)
  )
  u_block_b (
    .clk (clk),
    .in0 (data_i),
    .out0 (data_o));
endmodule
"#;

const PORTMAP_TRAILING_OUT: &str = r#"module top;
  block_b #(
    .PARAM_A (1),
    .PARAM_B (N-1)
  )
  u_block_b (
    .clk  (clk),
    .in0  (data_i),
    .out0 (data_o));
endmodule
"#;

// ---------------------------------------------------------------------------------
// structure literal fields and case items
// ---------------------------------------------------------------------------------

const FIELDS_IN: &str = r#"module top;
  always_comb begin
    x[i] = '{
            a: value_a[i],
            some_longer_field: value_b[i],
            c: some_type_t'(0)
            };
  end
endmodule
"#;

const FIELDS_OUT: &str = r#"module top;
  always_comb begin
    x[i] = '{
            a:                 value_a[i],
            some_longer_field: value_b[i],
            c:                 some_type_t'(0)
            };
  end
endmodule
"#;

const CASE_IN: &str = r#"module top;
  always_comb begin
    case ({a[i], b[i]})
    2'b10: next[i] = count[i] + 1'b1;
    2'b01: next[i] = count[i] - 1'b1;
    default: next[i] = count[i];
    endcase
  end
endmodule
"#;

const CASE_OUT: &str = r#"module top;
  always_comb begin
    case ({a[i], b[i]})
      2'b10:   next[i] = count[i] + 1'b1;
      2'b01:   next[i] = count[i] - 1'b1;
      default: next[i] = count[i];
    endcase
  end
endmodule
"#;

// ---------------------------------------------------------------------------------
// indentation
// ---------------------------------------------------------------------------------

const INDENT_IN: &str = r#"module m (
input wire clk
);
localparam int N = 1;
always_ff @(posedge clk) begin
if (a) begin
x <= 1;
end else begin
x <= 0;
end
end
generate
for (genvar i=0; i<N; i=i+1) begin : gen_i
assign y[i] = x;
end
endgenerate
endmodule
"#;

const INDENT_OUT: &str = r#"module m (
  input wire clk
);
  localparam int N = 1;
  always_ff @(posedge clk) begin
    if (a) begin
      x <= 1;
    end else begin
      x <= 0;
    end
  end
  generate
    for (genvar i=0; i<N; i=i+1) begin : gen_i
      assign y[i] = x;
    end
  endgenerate
endmodule
"#;

/// a single statement used as an if/assert body is indented once, and the
/// statement after it returns to the level of the if
const SINGLE_STATEMENT_BODY: &str = r#"module top;
  always_ff @(posedge clk) begin
    if (en)
      assert (a < LIMIT)
        else $error("value out of range");

    if (en && (MODE != 0))
      assert (a >= OFFSET)
        else $error("value too low");
  end
endmodule
"#;

const IF_ELSE_BODY: &str = r#"module top;
  always_comb begin
    if (rst)
      next = A;
    else
      next = B;
  end
endmodule
"#;

// ---------------------------------------------------------------------------------
// things the formatter must leave alone
// ---------------------------------------------------------------------------------

/// an inline attribute must not pad the lines that lack one -- the attribute
/// is the first column, so padding it would masquerade as indentation
const MIXED_ATTRIBUTES_IN: &str = r#"module top;
  logic a = 1'b0;
  (* max_fanout = 32 *) logic b = 1'b0;
  (* max_fanout = 32 *) logic b_next = 1'b0;
  logic c;
  logic d = 1'b0;
endmodule
"#;

const MIXED_ATTRIBUTES_OUT: &str = r#"module top;
  logic a = 1'b0;
  (* max_fanout = 32 *) logic b      = 1'b0;
  (* max_fanout = 32 *) logic b_next = 1'b0;
  logic c;
  logic d = 1'b0;
endmodule
"#;

/// a hand-aligned continuation of a ternary is not a case label
const TERNARY_CONTINUATION: &str = r#"module top;
  wire z = (P != 0) ? 1'b1 :
           (Q != 0) ? 1'b0 :
                       r;
endmodule
"#;

/// multi-line statements are not aligned
const UNTOUCHED: &str = r#"module top;
  assign z = f(a) +
    g(b);
endmodule
"#;

/// a declaration of several names shares the type column with its
/// neighbours, so it neither escapes the block nor breaks the run for the
/// lines below it
const DECL_LIST_IN: &str = r#"module top;
  logic [7:0] a;
  logic b;
  logic [3:0] c;
  logic [7:0] d;
  logic e = 1'b0;
  logic f = 1'b0;
  logic g, h;
  logic [3:0] i;
  logic [1:0] j, k, l;
  logic m;
endmodule
"#;

const DECL_LIST_OUT: &str = r#"module top;
  logic [7:0] a;
  logic       b;
  logic [3:0] c;
  logic [7:0] d;
  logic       e = 1'b0;
  logic       f = 1'b0;
  logic       g, h;
  logic [3:0] i;
  logic [1:0] j, k, l;
  logic       m;
endmodule
"#;

// ---------------------------------------------------------------------------------
// module headers: the parameter and port lists open on the header line
// ---------------------------------------------------------------------------------

const HEADER_REFLOW_IN: &str = r#"module top
  #(int PARAM_A=8,
    int PARAM_B=1
  ) (
  input logic clk,
  output logic q
);
endmodule
"#;

const HEADER_REFLOW_OUT: &str = r#"module top #(
  int PARAM_A = 8,
  int PARAM_B = 1
) (
  input logic  clk,
  output logic q
);
endmodule
"#;

/// a header whose lists both open and close on one line is left as written
const HEADER_ONE_LINE: &str = "module leaf #(int N = 8) (input logic clk);\nendmodule\n";

/// a port list that starts on the header line is pushed down, and a
/// dangling `(` with no parameter list is pulled up
const HEADER_PORTS_IN: &str = r#"module top
  (input logic clk,
   output logic q);
endmodule
"#;

const HEADER_PORTS_OUT: &str = r#"module top (
  input logic  clk,
  output logic q);
endmodule
"#;

const FORMAT_OFF: &str = r#"module m;
  // verilog-format: off
  logic     a;
  logic       bb;
  // verilog-format: on
  logic c;
  logic dd;
endmodule
"#;

/// comments and strings that contain formatter-relevant punctuation
const QUOTED: &str = r#"module m;
  initial begin
    /* a block comment
         with an odd    indent; and = signs
     */
    $display("a = %d; // not a comment", a);
  end
endmodule
"#;

macro_rules! case_tests {
    ($(($name:ident, $input:ident, $expected:ident)),+ $(,)?) => {
        $(
            mod $name {
                #[test]
                fn format() {
                    let got = super::format_default(crate::$input).expect("format_text should not error");
                    assert_eq!(got, crate::$expected);
                }

                #[test]
                fn idempotent() {
                    let once = super::format_default(crate::$input).unwrap();
                    let twice = super::format_default(&once).unwrap();
                    assert_eq!(twice, once);
                }

                #[test]
                fn tokens_preserved() {
                    let formatted = super::format_default(crate::$input).unwrap();
                    assert_eq!(super::token_stream(&formatted), super::token_stream(crate::$input));
                }

                /// Each pass alone must be a no-op on fully formatted text.
                ///
                /// The pre-commit hook runs --align-only while `make format`
                /// runs both, so a disagreement here means the two would
                /// undo each other's work.
                #[test]
                fn passes_agree_on_formatted_text() {
                    let formatted = super::format_default(crate::$input).unwrap();
                    let align_only = super::format_text(&formatted, &super::FormatOptions { do_indent: false, ..Default::default() }).unwrap();
                    assert_eq!(align_only, formatted);
                    let indent_only = super::format_text(&formatted, &super::FormatOptions { do_align: false, ..Default::default() }).unwrap();
                    assert_eq!(indent_only, formatted);
                }
            }
        )+
    };
}

case_tests! {
    (params, PARAMS_IN, PARAMS_OUT),
    (ports, PORTS_IN, PORTS_OUT),
    (header, HEADER_IN, HEADER_OUT),
    (variables, VARIABLES_IN, VARIABLES_OUT),
    (attributed_list, ATTRIBUTED_LIST_IN, ATTRIBUTED_LIST_OUT),
    (initialiser, INITIALISER_IN, INITIALISER_OUT),
    (r#struct, STRUCT_IN, STRUCT_OUT),
    (assign, ASSIGN_IN, ASSIGN_OUT),
    (mixed_ops, MIXED_OPS_IN, MIXED_OPS_OUT),
    (mixed_attributes, MIXED_ATTRIBUTES_IN, MIXED_ATTRIBUTES_OUT),
    (portmap, PORTMAP_IN, PORTMAP_OUT),
    (portmap_trailing, PORTMAP_TRAILING_IN, PORTMAP_TRAILING_OUT),
    (instance, INSTANCE_IN, INSTANCE_OUT),
    (instance_params, INSTANCE_PARAMS_IN, INSTANCE_PARAMS_OUT),
    (fields, FIELDS_IN, FIELDS_OUT),
    (r#case, CASE_IN, CASE_OUT),
    (indent, INDENT_IN, INDENT_OUT),
    (single_statement_body, SINGLE_STATEMENT_BODY, SINGLE_STATEMENT_BODY),
    (if_else_body, IF_ELSE_BODY, IF_ELSE_BODY),
    (ternary_continuation, TERNARY_CONTINUATION, TERNARY_CONTINUATION),
    (untouched, UNTOUCHED, UNTOUCHED),
    (decl_list, DECL_LIST_IN, DECL_LIST_OUT),
    (header_reflow, HEADER_REFLOW_IN, HEADER_REFLOW_OUT),
    (header_one_line, HEADER_ONE_LINE, HEADER_ONE_LINE),
    (header_ports, HEADER_PORTS_IN, HEADER_PORTS_OUT),
    (format_off, FORMAT_OFF, FORMAT_OFF),
    (quoted, QUOTED, QUOTED),
}

// ---------------------------------------------------------------------------------
// one-off behaviours
// ---------------------------------------------------------------------------------

#[test]
fn indent_only_keeps_comment_columns() {
    let source = "module m;\n  logic a;    // one\n  logic bb;   // two\nendmodule\n";
    let opts = FormatOptions {
        do_align: false,
        ..Default::default()
    };
    assert_eq!(format_text(source, &opts).unwrap(), source);
}

#[test]
fn align_only_keeps_indentation() {
    let mangled = "module m;\n        logic a;\n        logic bb;\nendmodule\n";
    let opts = FormatOptions {
        do_indent: false,
        ..Default::default()
    };
    assert_eq!(
        format_text(mangled, &opts).unwrap(),
        "module m;\n        logic a;\n        logic bb;\nendmodule\n"
    );
}

#[test]
fn indent_only_keeps_spacing() {
    let source = "module m;\nlogic a;\nlogic  bb;\nendmodule\n";
    let opts = FormatOptions {
        do_align: false,
        ..Default::default()
    };
    assert_eq!(
        format_text(source, &opts).unwrap(),
        "module m;\n  logic a;\n  logic  bb;\nendmodule\n"
    );
}

#[test]
fn indent_width_is_configurable() {
    let source = "module m;\nlogic a;\nendmodule\n";
    let opts = FormatOptions {
        unit: 4,
        ..Default::default()
    };
    assert_eq!(format_text(source, &opts).unwrap(), "module m;\n    logic a;\nendmodule\n");
}

#[test]
fn trailing_whitespace_is_removed() {
    assert_eq!(format_default("module m;   \n\nendmodule\n").unwrap(), "module m;\n\nendmodule\n");
}

#[test]
fn nested_brackets_in_an_lvalue_still_read_as_an_assignment() {
    let source = "module m;\n  always_comb begin\n    a[q[i]].m  =  s[i] ? (i == 0 ? U : D) : IDLE;\n  end\nendmodule\n";
    let expected = "module m;\n  always_comb begin\n    a[q[i]].m = s[i] ? (i == 0 ? U : D) : IDLE;\n  end\nendmodule\n";
    assert_eq!(format_default(source).unwrap(), expected);
}

#[test]
fn a_ternary_colon_is_not_a_label() {
    let source = "module m;\n  wire w = c ? (a) : b;\n  wire x = c ? (a) : b;\nendmodule\n";
    assert_eq!(format_default(source).unwrap(), source);
}

/// An escaped name runs to whitespace, so that space is not the formatter's
/// to close up. The verification pass has to notice, and refuse the file.
#[test]
fn an_escaped_identifier_keeps_the_space_that_ends_it() {
    let source = "module m;\n  wire \\<const0> ;\n  wire foo;\nendmodule\n";
    let error: Result<String, SvFormatError> = format_default(source);
    assert!(error.is_err());
}

#[test]
fn single_line_runs_collapse_to_one_space() {
    // one line on its own has nothing to align to, so it gets minimal spacing
    let source = "module m;\n  assign x  =  y;\nendmodule\n";
    assert_eq!(format_default(source).unwrap(), "module m;\n  assign x = y;\nendmodule\n");
}

#[test]
fn single_declaration_with_a_comment_collapses() {
    let source = "module m;\n  logic       a; // note\nendmodule\n";
    let expected = "module m;\n  logic a; // note\nendmodule\n";
    assert_eq!(format_default(source).unwrap(), expected);
}
