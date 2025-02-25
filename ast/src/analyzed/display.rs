//! Formatting functions for analyzed PIL files.
//!
//! These are not meant to be 1-1 reproductions, they will have errors.
//! Do not use this to re-generate PIL files!

use std::{
    fmt::{Display, Formatter, Result},
    str::FromStr,
};

use itertools::Itertools;

use crate::{parsed::FunctionKind, writeln_indented, writeln_indented_by};

use self::parsed::{
    asm::{AbsoluteSymbolPath, SymbolPath},
    display::format_type_scheme_around_name,
};

use super::*;

impl<T: Display> Display for Analyzed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let degree = self.degree.unwrap_or_default();
        let mut current_namespace = AbsoluteSymbolPath::default();
        let mut update_namespace = |name: &str, f: &mut Formatter<'_>| {
            let mut namespace =
                AbsoluteSymbolPath::default().join(SymbolPath::from_str(name).unwrap());
            let name = namespace.pop().unwrap();
            if namespace != current_namespace {
                current_namespace = namespace;
                writeln!(
                    f,
                    "namespace {}({degree});",
                    current_namespace.relative_to(&Default::default())
                )?;
            };
            Ok((name, !current_namespace.is_empty()))
        };

        for statement in &self.source_order {
            match statement {
                StatementIdentifier::Definition(name) => {
                    if self.auto_added_symbols.contains(name) {
                        // Core symbol added automatically, no need to print.
                        continue;
                    }
                    if let Some((symbol, definition)) = self.definitions.get(name) {
                        if matches!(
                            definition,
                            Some(FunctionValueDefinition::TypeConstructor(_, _))
                        ) {
                            // These are printed as part of the enum.
                            continue;
                        }
                        let (name, is_local) = update_namespace(name, f)?;
                        match symbol.kind {
                            SymbolKind::Poly(_) => {
                                writeln_indented(f, format_poly(&name, symbol, definition))?;
                            }
                            SymbolKind::Constant() => {
                                assert!(symbol.stage.is_none());
                                let Some(FunctionValueDefinition::Expression(TypedExpression {
                                    e,
                                    type_scheme,
                                })) = &definition
                                else {
                                    panic!(
                                        "Invalid constant value: {}",
                                        definition.as_ref().unwrap()
                                    );
                                };
                                assert!(
                                    type_scheme.is_none()
                                        || type_scheme == &Some((Type::Fe).into())
                                );
                                writeln_indented_by(
                                    f,
                                    format!("constant {name} = {e};"),
                                    is_local.into(),
                                )?;
                            }
                            SymbolKind::Other() => {
                                assert!(symbol.stage.is_none());
                                match definition {
                                    Some(FunctionValueDefinition::Expression(
                                        TypedExpression { e, type_scheme },
                                    )) => {
                                        writeln_indented(
                                            f,
                                            format!(
                                                "let{} = {e};",
                                                format_type_scheme_around_name(&name, type_scheme)
                                            ),
                                        )?;
                                    }
                                    Some(FunctionValueDefinition::TypeDeclaration(
                                        enum_declaration,
                                    )) => {
                                        writeln_indented(
                                            f,
                                            enum_declaration.to_string_with_name(&name),
                                        )?;
                                    }
                                    _ => {
                                        unreachable!("Invalid definition for symbol: {}", name)
                                    }
                                }
                            }
                        }
                    } else if let Some((symbol, definition)) = self.intermediate_columns.get(name) {
                        assert!(symbol.stage.is_none());
                        let (name, _) = update_namespace(name, f)?;
                        assert_eq!(symbol.kind, SymbolKind::Poly(PolynomialType::Intermediate));
                        if let Some(length) = symbol.length {
                            writeln_indented(
                                f,
                                format!(
                                    "col {name}[{length}] = [{}];",
                                    definition.iter().format(", ")
                                ),
                            )?;
                        } else {
                            assert_eq!(definition.len(), 1);
                            writeln_indented(f, format!("col {name} = {};", definition[0]))?;
                        }
                    } else {
                        panic!()
                    }
                }
                StatementIdentifier::PublicDeclaration(name) => {
                    let decl = &self.public_declarations[name];
                    let (name, is_local) = update_namespace(&decl.name, f)?;
                    writeln_indented_by(
                        f,
                        format_public_declaration(&name, decl),
                        is_local.into(),
                    )?;
                }
                StatementIdentifier::Identity(i) => {
                    writeln_indented(f, &self.identities[*i])?;
                }
            }
        }

        Ok(())
    }
}

fn format_poly(
    name: &str,
    symbol: &Symbol,
    definition: &Option<FunctionValueDefinition>,
) -> String {
    let SymbolKind::Poly(poly_type) = symbol.kind else {
        panic!()
    };
    let kind = match &poly_type {
        PolynomialType::Committed => "witness ",
        PolynomialType::Constant => "fixed ",
        PolynomialType::Intermediate => panic!(),
    };
    let stage = symbol
        .stage
        .map(|s| format!("stage({s}) "))
        .unwrap_or_default();
    let length = symbol
        .length
        .and_then(|length| {
            if let PolynomialType::Committed = poly_type {
                assert!(definition.is_none());
                Some(format!("[{length}]"))
            } else {
                // Do not print an array size, because we will do it as part of the type.
                assert!(matches!(
                    definition,
                    None | Some(FunctionValueDefinition::Expression(TypedExpression {
                        e: _,
                        type_scheme: Some(_)
                    }))
                ));
                None
            }
        })
        .unwrap_or_default();
    let value = definition
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    format!("col {kind}{stage}{name}{length}{value};")
}

fn format_public_declaration(name: &str, decl: &PublicDeclaration) -> String {
    format!(
        "public {name} = {}{}({});",
        decl.polynomial,
        decl.array_index
            .map(|i| format!("[{i}]"))
            .unwrap_or_default(),
        decl.index
    )
}

impl Display for FunctionValueDefinition {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            FunctionValueDefinition::Array(items) => {
                write!(f, " = {}", items.iter().format(" + "))
            }
            FunctionValueDefinition::Expression(TypedExpression {
                e,
                type_scheme: None,
            }) => format_outer_function(e, f),
            FunctionValueDefinition::Expression(TypedExpression {
                e,
                type_scheme: Some(ty),
            }) if *ty == Type::Col.into() => format_outer_function(e, f),
            FunctionValueDefinition::Expression(TypedExpression {
                e,
                type_scheme: Some(ts),
            }) => {
                assert!(ts.vars.is_empty(), "Should not have called this display function, since we cannot properly format the type vars.");
                write!(f, ": {} = {e}", ts.ty)
            }
            FunctionValueDefinition::TypeDeclaration(_)
            | FunctionValueDefinition::TypeConstructor(_, _) => {
                panic!("Should not use this formatting function.")
            }
        }
    }
}

fn format_outer_function(e: &Expression, f: &mut Formatter<'_>) -> Result {
    match e {
        parsed::Expression::LambdaExpression(_, lambda) if lambda.params.len() == 1 => {
            let body = if lambda.kind == FunctionKind::Pure
                && !matches!(lambda.body.as_ref(), Expression::BlockExpression(_, _))
            {
                format!("{{ {} }}", lambda.body)
            } else {
                format!("{}", lambda.body)
            };
            write!(
                f,
                "({}) {}{body}",
                lambda.params.iter().format(", "),
                match lambda.kind {
                    FunctionKind::Pure => "".into(),
                    _ => format!("{} ", &lambda.kind),
                },
            )
        }
        _ => write!(f, " = {e}"),
    }
}

impl Display for RepeatedArray {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.is_empty() {
            return Ok(());
        }
        write!(f, "[{}]", self.pattern.iter().format(", "))?;
        if self.is_repeated() {
            write!(f, "*")?;
        }
        Ok(())
    }
}

impl Display for Identity<Expression> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.kind {
            IdentityKind::Polynomial => {
                let (left, right) = self.as_polynomial_identity();
                let right = right
                    .as_ref()
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "0".into());
                write!(f, "{left} = {right};")
            }
            IdentityKind::Plookup => write!(f, "{} in {};", self.left, self.right),
            IdentityKind::Permutation => write!(f, "{} is {};", self.left, self.right),
            IdentityKind::Connect => write!(f, "{} connect {};", self.left, self.right),
        }
    }
}

impl<T: Display> Display for Identity<AlgebraicExpression<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.kind {
            IdentityKind::Polynomial => {
                let (left, right) = self.as_polynomial_identity();
                let right = right
                    .as_ref()
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "0".into());
                write!(f, "{left} = {right};")
            }
            IdentityKind::Plookup => write!(f, "{} in {};", self.left, self.right),
            IdentityKind::Permutation => write!(f, "{} is {};", self.left, self.right),
            IdentityKind::Connect => write!(f, "{} connect {};", self.left, self.right),
        }
    }
}

impl<Expr: Display> Display for SelectedExpressions<Expr> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "{}{{ {} }}",
            self.selector
                .as_ref()
                .map(|s| format!("{s} "))
                .unwrap_or_default(),
            self.expressions.iter().format(", ")
        )
    }
}

impl Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reference::LocalVar(_index, name) => {
                write!(f, "{name}")
            }
            Reference::Poly(r) => write!(f, "{r}"),
        }
    }
}

impl<T: Display> Display for AlgebraicExpression<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            AlgebraicExpression::Reference(reference) => write!(f, "{reference}"),
            AlgebraicExpression::PublicReference(name) => write!(f, ":{name}"),
            AlgebraicExpression::Challenge(challenge) => {
                write!(
                    f,
                    "std::prover::challenge({}, {})",
                    challenge.stage, challenge.id,
                )
            }
            AlgebraicExpression::Number(value) => write!(f, "{value}"),
            AlgebraicExpression::BinaryOperation(o) => {
                write!(f, "{o}")
            }
            AlgebraicExpression::UnaryOperation(o) => write!(f, "{o}"),
        }
    }
}

impl<T: Display> Display for AlgebraicBinaryOperation<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let force_parentheses = matches!(self.op, AlgebraicBinaryOperator::Pow);

        let op_precedence = self.op.precedence().unwrap();
        let use_left_parentheses = match self.left.precedence() {
            Some(left_precedence) => {
                force_parentheses
                    || left_precedence > op_precedence
                    || (left_precedence == op_precedence
                        && self.op.associativity() != AlgebraicBinaryOperatorAssociativity::Left)
            }
            None => false,
        };

        let use_right_parentheses = match self.right.precedence() {
            Some(right_precedence) => {
                force_parentheses
                    || right_precedence > op_precedence
                    || (right_precedence == op_precedence
                        && self.op.associativity() != AlgebraicBinaryOperatorAssociativity::Right)
            }
            None => false,
        };

        let left_string = if use_left_parentheses {
            format!("({})", self.left)
        } else {
            format!("{}", self.left)
        };
        let right_string = if use_right_parentheses {
            format!("({})", self.right)
        } else {
            format!("{}", self.right)
        };

        write!(f, "{left_string} {} {right_string}", self.op)
    }
}

impl<T: Display> Display for AlgebraicUnaryOperation<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let exp_string = match (self.op.precedence(), self.expr.precedence()) {
            (Some(precedence), Some(inner_precedence)) if precedence < inner_precedence => {
                format!("({})", self.expr)
            }
            _ => {
                format!("{}", self.expr)
            }
        };

        if self.op.is_prefix() {
            write!(f, "{}{exp_string}", self.op)
        } else {
            write!(f, "{exp_string}{}", self.op)
        }
    }
}

impl Display for AlgebraicUnaryOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        UnaryOperator::from(*self).fmt(f)
    }
}

impl Display for AlgebraicBinaryOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        BinaryOperator::from(*self).fmt(f)
    }
}

impl Display for AlgebraicReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name, if self.next { "'" } else { "" },)
    }
}

impl Display for PolynomialReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.name)?;
        if let Some(type_args) = &self.type_args {
            if !type_args.is_empty() {
                write!(f, "::<{}>", type_args.iter().join(", "))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::iter::once;

    use powdr_number::GoldilocksField;
    use powdr_pil_analyzer::analyze_string;
    use pretty_assertions::assert_eq;
    use test_log::test;

    use super::{AlgebraicBinaryOperator, AlgebraicExpression};

    type TestCase<'a> = (&'a str, &'a str);

    fn test_paren(to_declare: &[&str], (input, expected): &TestCase) {
        // Display on `Analysis` pads each line by 4 spaces
        let padding: String = " ".repeat(4);

        // Introduce witness columns for the referenced variables
        let declarations = to_declare
            .iter()
            .map(|to_declare| format!("col witness {to_declare};"));

        // Wrap the expression we're testing in a polynomial identity
        let wrap = |e| {
            declarations
                .clone()
                .chain(once(format!("{e} = 0;")))
                .collect::<Vec<_>>()
                .join(&format!("\n{padding}"))
        };

        let (input, expected) = &(&wrap(input), &wrap(expected));
        let analyzed = analyze_string::<GoldilocksField>(input);
        let printed = analyzed.to_string();

        assert_eq!(expected.trim(), printed.trim());
    }

    #[test]
    fn exp_assoc() {
        // we test this separately from other expressions, since although `x ** y ** z` is allowed in `AlgebraicExpression`, it is not produced by the analyzer due to type system restrictions

        let x = AlgebraicExpression::Reference(super::AlgebraicReference {
            name: "x".into(),
            poly_id: super::PolyID {
                id: 0,
                ptype: super::PolynomialType::Committed,
            },
            next: false,
        });
        let y = AlgebraicExpression::Reference(super::AlgebraicReference {
            name: "y".into(),
            poly_id: super::PolyID {
                id: 1,
                ptype: super::PolynomialType::Committed,
            },
            next: false,
        });
        let z = AlgebraicExpression::Reference(super::AlgebraicReference {
            name: "z".into(),
            poly_id: super::PolyID {
                id: 2,
                ptype: super::PolynomialType::Committed,
            },
            next: false,
        });

        // define `x ** (y ** z)`
        let x_yz: AlgebraicExpression<GoldilocksField> = AlgebraicExpression::new_binary(
            x.clone(),
            AlgebraicBinaryOperator::Pow,
            AlgebraicExpression::new_binary(y.clone(), AlgebraicBinaryOperator::Pow, z.clone()),
        );
        // In principle, no parentheses needed as `**` is right-associative. However, we keep parentheses to match behavior of the parsed AST.
        assert_eq!(x_yz.to_string(), "x ** (y ** z)");

        // define `(x ** y) ** z`
        let xy_z = AlgebraicExpression::new_binary(
            AlgebraicExpression::new_binary(x.clone(), AlgebraicBinaryOperator::Pow, y.clone()),
            AlgebraicBinaryOperator::Pow,
            z.clone(),
        );
        // parentheses needed because `**` is right-associative
        assert_eq!(xy_z.to_string(), "(x ** y) ** z");
    }

    #[test]
    fn binary_op() {
        let test_cases: Vec<TestCase> = vec![
            // Don't add extra
            ("x + y + z", "x + y + z"),
            ("x * y * z", "x * y * z"),
            ("x ** 2", "x ** 2"),
            ("x ** 2 * y", "x ** 2 * y"),
            ("x * y ** 2", "x * y ** 2"),
            // Remove unneeded
            ("(-x) + y * (z)", "-x + y * z"),
            ("(x * y) * z", "x * y * z"),
            ("(x - (y + z))", "x - (y + z)"),
            ("(x ** 2)", "x ** 2"),
            ("(x ** 2) * y", "x ** 2 * y"),
            ("x * (y ** 2)", "x * y ** 2"),
            // Observe associativity
            ("x * (y * z)", "x * (y * z)"),
            ("x + (y + z)", "x + (y + z)"),
            // Don't remove needed
            ("(x + y) * z", "(x + y) * z"),
            ("((x + y) * z)", "(x + y) * z"),
            ("-(x + y)", "-(x + y)"),
            ("(x + y) ** 2", "(x + y) ** 2"),
        ];

        for test_case in test_cases {
            test_paren(&["x", "y", "z"], &test_case);
        }
    }

    #[test]
    fn access() {
        let array_test_cases: Vec<TestCase> = vec![
            ("-x[2]", "-x[2]"),
            ("-(x[2])", "-x[2]"),
            ("1 + x[2]", "1 + x[2]"),
        ];

        for test_case in array_test_cases {
            test_paren(&["x[42]"], &test_case);
        }
    }
}
