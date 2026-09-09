#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FormBackspace,
    FormInput(char),
    FormNextChoice,
    FormNextField,
    FormPreviousChoice,
    FormPreviousField,
    FormSubmit,
    Quit,
    ScrollEnd,
    ScrollPageDown,
    ScrollPageUp,
    ScrollStart,
}
