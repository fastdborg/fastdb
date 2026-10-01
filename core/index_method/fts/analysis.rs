use crate::LimboError;
use tantivy::tokenizer::{
    NgramTokenizer, RawTokenizer, SimpleTokenizer, TextAnalyzer, TokenStream, TokenizerManager,
    WhitespaceTokenizer,
};

pub(super) fn register(tokenizers: &TokenizerManager, window: (usize, usize)) {
    tokenizers.register("raw", RawTokenizer::default());
    tokenizers.register("simple", SimpleTokenizer::default());
    tokenizers.register("whitespace", WhitespaceTokenizer::default());
    if let Ok(ngram) = NgramTokenizer::new(window.0, window.1, false) {
        let analyzer = TextAnalyzer::builder(ngram)
            .filter(tantivy::tokenizer::LowerCaser)
            .build();
        tokenizers.register("ngram", analyzer);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnalyzedToken<'a> {
    pub text: &'a str,
    pub offset_from: usize,
    pub offset_to: usize,
    pub position: usize,
    pub position_length: usize,
}

pub fn analyze_text<E: From<LimboError>>(
    tokenizer: &str,
    ngram_window: (usize, usize),
    text: &str,
    mut visit: impl FnMut(AnalyzedToken<'_>) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    if !super::SUPPORTED_TOKENIZERS.contains(&tokenizer) {
        return Err(
            LimboError::ParseError(format!("unsupported FTS tokenizer '{tokenizer}'")).into(),
        );
    }
    let tokenizers = TokenizerManager::default();
    register(&tokenizers, ngram_window);
    let mut analyzer = tokenizers
        .get(tokenizer)
        .ok_or_else(|| LimboError::ParseError("invalid FTS tokenizer configuration".into()))?;
    let mut stream = analyzer.token_stream(text);
    while stream.advance() {
        let token = stream.token();
        visit(AnalyzedToken {
            text: &token.text,
            offset_from: token.offset_from,
            offset_to: token.offset_to,
            position: token.position,
            position_length: token.position_length,
        })?;
    }
    Ok(())
}
