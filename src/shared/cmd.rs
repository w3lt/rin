use crate::display;
use crate::shared::sender;
use anyhow::Result;

pub async fn send_message(team: Option<&str>, to: String, text: &[String]) -> Result<()> {
    let (client, hops) = sender(team)?;
    println!("{}", client.send(&to, &text.join(" "), hops).await?);
    Ok(())
}

pub async fn show_agents(team: Option<&str>) -> Result<()> {
    let (client, _) = sender(team)?;
    print!("{}", display::agents(&client.agents().await?));
    Ok(())
}
