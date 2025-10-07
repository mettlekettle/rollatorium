use thiserror::Error;

#[derive(Debug, Error)]
pub enum RollatoriumError {
    #[error("Lexer error: {0}")]
    Lexer(String),
    #[error("Parser error: {0}")]
    Parser(String),
    #[error("Evaluation error: {0}")]
    Eval(String),
}
