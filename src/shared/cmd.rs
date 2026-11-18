use crate::display;
use crate::shared::sender;
use anyhow::Result;

pub async fn send_message(team: Option<&str>, to: String, text: &[String]) -> Result<()> {
    let (client, hops) = sender(team)?;
    let reply = client.send(&to, &text.join(" "), hops).await?;
    display::out(&format!("{reply}\n"))?;
    Ok(())
}

pub async fn show_agents(team: Option<&str>) -> Result<()> {
    let (client, _) = sender(team)?;
    display::out(&display::agents(&client.agents().await?))?;
    Ok(())
}
