use ai_lib::{prompt, AiFunctionResult, AiFunctionResponse, AiInitialState, drive, recoverable_err, done};
use ai_macros::ai_functions;
use schemars::JsonSchema;
use serde::Deserialize;
use ansi_term::Color;

macro_rules! orange {
    ($($text:tt)*) => {
        println!("{}", Color::Fixed(214).paint(format_args!( $($text)* ).to_string()))
    }
}

macro_rules! blue {
    ($($text:tt)*) => {
        println!("{}", Color::Fixed(81).paint(format_args!( $($text)* ).to_string()))
    }
}


/*
#[tokio::main]
async fn main() {
    let mut example = SimpleExample::default();
    drive(&mut example).await.unwrap();
}
*/

#[derive(Debug, Default)]
struct SimpleExample {
    topic: String,
    random_words: Vec<String>,
}

impl AiInitialState for SimpleExample {
    fn initial(&mut self) -> AiFunctionResponse {
        prompt!(0.8, "Write a random topic for a story" => [write_topic])
    }
}

#[ai_functions]
impl SimpleExample {

    #[ai_function]
    fn write_topic(&mut self, topic: String) -> AiFunctionResult {
        // Print out the topic
        orange!("{}\n", topic);

        // Update state and then prompt to write random words
        self.topic = topic.clone();
        prompt!(0.5, "Write a list of random words that could be used in a story about {topic}" => [write_random_words])
    }

    #[ai_function]
    fn write_random_words(&mut self, random_words: Vec<String>) -> AiFunctionResult {
        // Print out the random words
        for word in &random_words {
            blue!("{word} ");
        }
        orange!("\n");

        // Update state and then prompt to write a paragraph with those words
        self.random_words = random_words;
        let topic = &self.topic;
        let random_words = self.random_words.join(", ");
        prompt!(0.5, "Write a paragraph about {topic} using the following random words: {random_words}" => [write_paragraph])
    }

    #[ai_function]
    fn write_paragraph(&mut self, paragraph: String) -> AiFunctionResult {
        orange!("{}\n", paragraph);
        done()
    }
}

#[tokio::main]
async fn main() {
    let mut story = Story::new("an alternate history in which the Maya defeat the Spanish with advanced but historically plausible technology, e.g. catapults, ships, fortresses, etc.");
    drive(&mut story).await.unwrap();
}

#[derive(Debug, Default)]
struct Story {
    topic: String,
    premise: String,
    premise_edits_remaining: u32,
    chapter_summaries: Vec<String>,
}

impl Story {
    fn new(topic: &str) -> Self {
        Self { topic: topic.into(), premise_edits_remaining: 3, ..Default::default() }
    }
}

impl AiInitialState for Story {
    fn initial(&mut self) -> AiFunctionResponse {
        let topic = &self.topic;
        prompt!(0.8, "Write a high-level story premise about the following topic. Use it as inspiration, but liberally expand on it. Topic: {topic}" => [write_premise])
    }
}

#[ai_functions]
impl Story {

    #[ai_function(fn_description="Write a story premise", notes="Scratch notes where you ideate")]
    fn write_premise(&mut self, notes: Vec<String>, premise: String) -> AiFunctionResult {
        // Print out chain of thoughts then the premise
        for (i, note) in notes.iter().enumerate() {
            blue!("{i}. {note}");
        }
        orange!("{}\n", premise);

        // Update state and then prompt to edit with medium temperature
        self.premise = premise.clone();
        let topic = &self.topic;
        prompt!(0.5, "Liberally edit this story premise Be detailed. Topic: {topic}\nPremise:{premise}" => [edit_premise])
    }

    #[ai_function(fn_description="Edit a story premise", notes = "Notes about what could be improved")]
    fn edit_premise(&mut self, notes: Vec<String>, rewritten_premise: String) -> AiFunctionResult {
        // Print out chain of thoughts then the premise
        for (i, note) in notes.iter().enumerate() {
            blue!("{i}. {note}");
        }
        orange!("{}\n", rewritten_premise);

        // Update state and then prompt to edit with medium temperature, or move on to chapter outlines
        // after a few rounds of editing
        self.premise = rewritten_premise.clone();
        self.premise_edits_remaining -= 1;

        let topic = &self.topic;
        if self.premise_edits_remaining == 0 {
            prompt!(0.5, "Write a detailed plot outline for each chapter of a story loosely based on this premise. Topic: {topic}\nPremise: {rewritten_premise}" => [write_chapter_outlines])
        } else {
            prompt!(0.5, "Liberally edit the following story premise. Be detailed. Topic: {topic}\nPremise: {rewritten_premise}" => [edit_premise])
        }
    }

    #[ai_function(fn_description="Write chapter outlines", outlines="List of detailed outlines for each chapter")]
    fn write_chapter_outlines(&mut self, outlines: Vec<String>) -> AiFunctionResult {

        // Print out chapter outlines, re-prompt if they're too short
        for outline in &outlines {
            if outline.len() < 80 {
                // Sometimes GPT gives only chapter titles, tell it to do better
                return recoverable_err(format!("Chapter outlines should be a few sentences at least, but this one was only {} characters long: {}. Write longer outlines for each chapter.", outline.len(), outline));
            }
            println!("{outline}\n");
        }

        self.chapter_summaries = outlines.clone();
        done()
    }
}
