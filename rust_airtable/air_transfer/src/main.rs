use reqwest::Error as ReqwestError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, Write};

#[derive(Debug)]
enum CustomError {
    Reqwest(ReqwestError),
    IO(std::io::Error),
    ParseInt(std::num::ParseIntError),
    Airtable(String),
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomError::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            CustomError::IO(e) => write!(f, "IO error: {}", e),
            CustomError::ParseInt(e) => write!(f, "ParseInt error: {}", e),
            CustomError::Airtable(e) => write!(f, "Airtable error: {}", e),
        }
    }
}

impl std::error::Error for CustomError {}

impl From<ReqwestError> for CustomError {
    fn from(e: ReqwestError) -> Self {
        CustomError::Reqwest(e)
    }
}

impl From<std::io::Error> for CustomError {
    fn from(e: std::io::Error) -> Self {
        CustomError::IO(e)
    }
}

impl From<std::num::ParseIntError> for CustomError {
    fn from(e: std::num::ParseIntError) -> Self {
        CustomError::ParseInt(e)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    Title: String,
    Body: String,
    Task: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct AirtableRecord {
    id: Option<String>,
    fields: Post,
}

#[derive(Serialize, Deserialize, Debug)]
struct AirtableResponse {
    records: Vec<AirtableRecord>,
}

const AIRTABLE_PERSONAL_ACCESS_TOKEN: &str =
    "patT5M1NvhyB5YMU3.7361c39c29738053459dabd03d3b420efa1ad8e1c29cbb10f4bae2b51c16d149";
const AIRTABLE_BASE_ID: &str = "appLMv58EvJqp11pl";
const AIRTABLE_TABLE_NAME: &str = "Posts";
const AIRTABLE_COPY_TABLE_NAME: &str = "Posts copy";

#[tokio::main]
async fn main() -> Result<(), CustomError> {
    loop {
        println!("Choose an operation:");
        println!("1. Create a post");
        println!("2. Read all posts");
        println!("3. Update a post");
        println!("4. Delete a post");
        println!("5. Transfer data to 'Posts copy'");
        println!("6. Exit");

        let mut choice = String::new();
        print!("Enter your choice: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut choice)?;
        let choice: u32 = match choice.trim().parse() {
            Ok(num) => num,
            Err(_) => {
                println!("Invalid input. Please enter a number.");
                continue;
            }
        };

        match choice {
            1 => create_post_interactive().await?,
            2 => read_posts().await?,
            3 => update_post_interactive().await?,
            4 => delete_post_interactive().await?,
            5 => transfer_data_to_copy_table().await?,
            6 => break,
            _ => println!("Invalid choice. Please try again."),
        }
    }

    Ok(())
}

async fn transfer_data_to_copy_table() -> Result<(), CustomError> {
    println!("Fetching data from 'Posts' table...");
    let posts = get_posts().await?;

    if posts.is_empty() {
        println!("No posts found to transfer.");
        return Ok(());
    }

    println!("Enter the title of the post you want to transfer:");
    let mut title = String::new();
    io::stdin().read_line(&mut title)?;
    let title = title.trim();

    // Find the post with the matching title
    let post_to_transfer = posts
        .into_iter()
        .find(|post| post.Title == title)
        .ok_or_else(|| CustomError::Airtable(format!("Post with title '{}' not found.", title)))?;

    println!("Found post: {:?}", post_to_transfer);

    println!("Do you want to transfer this post to 'Posts copy'? (yes/no)");
    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;

    if confirmation.trim().to_lowercase() != "yes" {
        println!("Transfer cancelled.");
        return Ok(());
    }

    println!("Transferring post to 'Posts copy' table...");
    create_post_in_table(&post_to_transfer, AIRTABLE_COPY_TABLE_NAME).await?;

    println!("Post transferred successfully.");
    Ok(())
}

async fn create_post_in_table(post: &Post, table_name: &str) -> Result<(), CustomError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        AIRTABLE_BASE_ID, table_name
    );

    let payload = serde_json::json!({
        "records": [
            {
                "fields": {
                    "Title": post.Title.clone(),
                    "Body": post.Body.clone(),
                    "Task": post.Task,
                }
            }
        ]
    });

    let response = client
        .post(&url)
        .json(&payload)
        .header(
            "Authorization",
            format!("Bearer {}", AIRTABLE_PERSONAL_ACCESS_TOKEN),
        )
        .send()
        .await?;

    if !response.status().is_success() {
        let error_message = response.text().await?;
        return Err(CustomError::Airtable(format!(
            "Failed to create post in table '{}': {}",
            table_name, error_message
        )));
    }

    Ok(())
}

async fn create_post_interactive() -> Result<(), CustomError> {
    let mut title = String::new();
    let mut body = String::new();
    let mut task = String::new();

    print!("Enter post title: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut title)?;

    print!("Enter post body: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut body)?;

    print!("Enter task: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut task)?;
    let task: i32 = task.trim().parse()?;

    let new_post = Post {
        Title: title.trim().to_string(),
        Body: body.trim().to_string(),
        Task: task,
    };

    create_post(&new_post).await?;
    println!("Post created successfully.");

    Ok(())
}

async fn read_posts() -> Result<(), CustomError> {
    let posts = get_posts().await?;
    println!("All Posts: {:?}", posts);
    Ok(())
}

async fn update_post_interactive() -> Result<(), CustomError> {
    let mut id = String::new();
    let mut title = String::new();
    let mut body = String::new();
    let mut task = String::new();

    print!("Enter post ID to update: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut id)?;
    let id = id.trim();

    print!("Enter new post title: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut title)?;

    print!("Enter new post body: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut body)?;

    print!("Enter new task: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut task)?;
    let task: i32 = task.trim().parse()?;

    let updated_post = Post {
        Title: title.trim().to_string(),
        Body: body.trim().to_string(),
        Task: task,
    };

    update_post(id, &updated_post).await?;
    println!("Post updated successfully.");

    Ok(())
}

async fn delete_post_interactive() -> Result<(), CustomError> {
    let mut id = String::new();

    print!("Enter post ID to delete: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut id)?;
    let id = id.trim();

    delete_post(id).await?;
    println!("Post with ID {} deleted.", id);

    Ok(())
}

async fn create_post(post: &Post) -> Result<(), CustomError> {
    create_post_in_table(post, AIRTABLE_TABLE_NAME).await
}

async fn get_posts() -> Result<Vec<Post>, CustomError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        AIRTABLE_BASE_ID, AIRTABLE_TABLE_NAME
    );

    let response = client
        .get(&url)
        .header(
            "Authorization",
            format!("Bearer {}", AIRTABLE_PERSONAL_ACCESS_TOKEN),
        )
        .send()
        .await?;

    if !response.status().is_success() {
        let error_message = response.text().await?;
        return Err(CustomError::Airtable(format!(
            "Failed to fetch posts: {}",
            error_message
        )));
    }

    let airtable_response: AirtableResponse = response.json().await?;
    let posts = airtable_response
        .records
        .into_iter()
        .map(|record| record.fields)
        .collect();

    Ok(posts)
}

async fn update_post(id: &str, post: &Post) -> Result<(), CustomError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}/{}",
        AIRTABLE_BASE_ID, AIRTABLE_TABLE_NAME, id
    );

    let payload = serde_json::json!({
        "fields": {
            "Title": post.Title.clone(),
            "Body": post.Body.clone(),
            "Task": post.Task,
        }
    });

    let response = client
        .patch(&url)
        .json(&payload)
        .header(
            "Authorization",
            format!("Bearer {}", AIRTABLE_PERSONAL_ACCESS_TOKEN),
        )
        .send()
        .await?;

    if !response.status().is_success() {
        let error_message = response.text().await?;
        return Err(CustomError::Airtable(format!(
            "Failed to update post: {}",
            error_message
        )));
    }

    Ok(())
}

async fn delete_post(id: &str) -> Result<(), CustomError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}/{}",
        AIRTABLE_BASE_ID, AIRTABLE_TABLE_NAME, id
    );

    let response = client
        .delete(&url)
        .header(
            "Authorization",
            format!("Bearer {}", AIRTABLE_PERSONAL_ACCESS_TOKEN),
        )
        .send()
        .await?;

    if !response.status().is_success() {
        let error_message = response.text().await?;
        return Err(CustomError::Airtable(format!(
            "Failed to delete post: {}",
            error_message
        )));
    }

    Ok(())
}