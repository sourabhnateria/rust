use reqwest::Error as ReqwestError;
use serde::{Deserialize, Serialize};
use mongodb::{Client, options::ClientOptions};
use mongodb::bson::{doc, Document, oid::ObjectId};
use std::io::{self, Write};
use std::fmt;

#[derive(Debug)]
enum CustomError {
    MongoDB(mongodb::error::Error),
    ObjectId(mongodb::bson::oid::Error),
    IO(std::io::Error),
    Reqwest(ReqwestError),
    ParseInt(std::num::ParseIntError),
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomError::MongoDB(e) => write!(f, "MongoDB error: {}", e),
            CustomError::ObjectId(e) => write!(f, "ObjectId error: {}", e),
            CustomError::IO(e) => write!(f, "IO error: {}", e),
            CustomError::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            CustomError::ParseInt(e) => write!(f, "ParseInt error: {}", e),
        }
    }
}

impl std::error::Error for CustomError {}

impl From<mongodb::error::Error> for CustomError {
    fn from(e: mongodb::error::Error) -> Self {
        CustomError::MongoDB(e)
    }
}

impl From<mongodb::bson::oid::Error> for CustomError {
    fn from(e: mongodb::bson::oid::Error) -> Self {
        CustomError::ObjectId(e)
    }
}

impl From<std::io::Error> for CustomError {
    fn from(e: std::io::Error) -> Self {
        CustomError::IO(e)
    }
}

impl From<ReqwestError> for CustomError {
    fn from(e: ReqwestError) -> Self {
        CustomError::Reqwest(e)
    }
}

impl From<std::num::ParseIntError> for CustomError {
    fn from(e: std::num::ParseIntError) -> Self {
        CustomError::ParseInt(e)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    title: String,
    body: String,
    task: i32,
}

#[tokio::main]
async fn main() -> Result<(), CustomError> {
    loop {
        println!("Choose an operation:");
        println!("1. Create a post");
        println!("2. Read all posts");
        println!("3. Update a post");
        println!("4. Delete a post");
        println!("5. Exit");

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
            5 => break,
            _ => println!("Invalid choice. Please try again."),
        }
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

    print!("Enter task : ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut task)?;
    let task: i32 = task.trim().parse()?;

    let new_post = Post {
        title: title.trim().to_string(),
        body: body.trim().to_string(),
        task: task,
    };

    let created_post = create_post(&new_post).await?;
    println!("Created Post: {:?}", created_post);

    save_to_mongodb(&new_post).await?;
    println!("Post saved to MongoDB.");

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
        title: title.trim().to_string(),
        body: body.trim().to_string(),
        task: task,
    };

    update_post_in_mongodb(id, &updated_post).await?;
    println!("Post updated successfully.");

    Ok(())
}

async fn delete_post_interactive() -> Result<(), CustomError> {
    let mut id = String::new();

    print!("Enter post ID to delete: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut id)?;
    let id = id.trim();

    delete_post_in_mongodb(id).await?;
    println!("Post with ID {} deleted.", id);

    Ok(())
}

async fn create_post(post: &Post) -> Result<Post, ReqwestError> {
    let client = reqwest::Client::new();
    let res = client.post("https://jsonplaceholder.typicode.com/posts")
        .json(post)
        .send()
        .await?
        .json::<Post>()
        .await?;
    Ok(res)
}

async fn get_posts() -> Result<Vec<Post>, ReqwestError> {
    let res = reqwest::get("https://jsonplaceholder.typicode.com/posts")
        .await?
        .json::<Vec<Post>>()
        .await?;
    Ok(res)
}

async fn update_post_in_mongodb(id: &str, post: &Post) -> Result<(), CustomError> {
    let client_options = ClientOptions::parse("mongodb://localhost:27017").await?;
    let client = Client::with_options(client_options)?;
    let db = client.database("rust_crud_db");
    let collection = db.collection::<Document>("posts");

    // Convert the string ID to ObjectId
    let object_id = ObjectId::parse_str(id)?;

    let filter = doc! { "_id": object_id };
    let update = doc! {
        "$set": {
            "title": &post.title,
            "body": &post.body,
            "task": post.task,
        }
    };

    collection.update_one(filter, update, None).await?;
    Ok(())
}

async fn delete_post_in_mongodb(id: &str) -> Result<(), CustomError> {
    let client_options = ClientOptions::parse("mongodb://localhost:27017").await?;
    let client = Client::with_options(client_options)?;
    let db = client.database("rust_crud_db");
    let collection = db.collection::<Document>("posts");

    // Convert the string ID to ObjectId
    let object_id = ObjectId::parse_str(id)?;

    let filter = doc! { "_id": object_id };
    collection.delete_one(filter, None).await?;
    Ok(())
}

async fn save_to_mongodb(post: &Post) -> Result<(), CustomError> {
    let client_options = ClientOptions::parse("mongodb://localhost:27017").await?;
    let client = Client::with_options(client_options)?;
    let db = client.database("rust_crud_db");
    let collection = db.collection::<Document>("posts");

    let post_doc = doc! {
        "title": &post.title,
        "body": &post.body,
        "task": post.task,
    };

    collection.insert_one(post_doc, None).await?;
    Ok(())
}